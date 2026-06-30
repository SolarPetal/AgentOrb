use std::{
    env, fmt, fs, io,
    net::IpAddr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use agent_orb_core::{
    config::Config,
    event::{EventEnvelope, EventType},
    source::Source,
};
use clap::{Parser, Subcommand};
use serde_json::json;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    process::Command,
    sync::watch,
    task::JoinError,
    time::{sleep, Instant},
};
use uuid::Uuid;

const TOKEN_FILE_NAME: &str = "token";

#[derive(Debug, Parser)]
#[command(name = "agent_orb", version, about = "Agent Orb CLI wrapper")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run a target CLI through Agent Orb.
    Run {
        /// Command and arguments after `--`.
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let exit_code = match run_main().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("agent_orb: {err}");
            1
        }
    };

    std::process::exit(exit_code);
}

async fn run_main() -> Result<i32, AppError> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Run { command }) => run_wrapped_command(command).await,
        None => {
            println!("Agent Orb CLI. Try: agent_orb run -- echo hello");
            Ok(0)
        }
    }
}

async fn run_wrapped_command(command: Vec<String>) -> Result<i32, AppError> {
    if command.is_empty() {
        return Err(AppError::EmptyCommand);
    }

    let config_dir = default_config_dir();
    let config = Config::load_from_dir_or_default(&config_dir);
    ensure_loopback_host(&config.daemon.host)?;
    ensure_daemon_running(&config).await?;
    let token = read_token(&config_dir)?;
    let client = DaemonClient::new(config.daemon.host.clone(), config.daemon.port, token);

    let source = detect_source(&command[0]);
    let workspace = env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| ".".to_string());
    let session_id = Uuid::now_v7().to_string();
    let started_at = OffsetDateTime::now_utc();

    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| AppError::Spawn {
            command: command[0].clone(),
            source,
        })?;

    let pid = child.id();
    let started_event = build_event(
        &session_id,
        source.clone(),
        workspace.clone(),
        EventType::SessionStarted,
        json!({
            "command": shell_join(&command),
            "pid": pid,
            "platform": env::consts::OS,
        }),
    );
    warn_if_event_failed(client.post_event(&started_event).await, "session.started");

    let prompt_detector = Arc::new(PromptDetector::for_source(&source));
    let (activity_tx, activity_rx) = watch::channel(Instant::now());
    let timeout_task = tokio::spawn(monitor_timeouts(
        client.clone(),
        session_id.clone(),
        source.clone(),
        workspace.clone(),
        config.behavior.silent_threshold_seconds,
        config.behavior.stuck_threshold_seconds,
        activity_rx,
    ));

    let stdout_task = child.stdout.take().map(|stdout| {
        tokio::spawn(forward_stream(
            stdout,
            tokio::io::stdout(),
            "stdout",
            client.clone(),
            session_id.clone(),
            source.clone(),
            workspace.clone(),
            config.privacy.include_output_sample,
            config.privacy.max_sample_chars,
            prompt_detector.clone(),
            activity_tx.clone(),
        ))
    });

    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(forward_stream(
            stderr,
            tokio::io::stderr(),
            "stderr",
            client.clone(),
            session_id.clone(),
            source.clone(),
            workspace.clone(),
            config.privacy.include_output_sample,
            config.privacy.max_sample_chars,
            prompt_detector.clone(),
            activity_tx.clone(),
        ))
    });

    let status = child.wait().await.map_err(AppError::Io)?;

    if let Some(task) = stdout_task {
        join_stream_task(task).await?;
    }
    if let Some(task) = stderr_task {
        join_stream_task(task).await?;
    }
    drop(activity_tx);
    timeout_task.await.map_err(AppError::Join)?;

    let exit_code = status.code().unwrap_or(1);
    let duration_ms = (OffsetDateTime::now_utc() - started_at).whole_milliseconds();
    let exited_event = build_event(
        &session_id,
        source,
        workspace,
        EventType::ProcessExited,
        json!({
            "exit_code": exit_code,
            "duration_ms": duration_ms.max(0),
        }),
    );
    warn_if_event_failed(client.post_event(&exited_event).await, "process.exited");

    Ok(exit_code)
}

async fn forward_stream<R, W>(
    mut reader: R,
    mut writer: W,
    stream_name: &'static str,
    client: DaemonClient,
    session_id: String,
    source: Source,
    workspace: String,
    include_output_sample: bool,
    max_sample_chars: usize,
    prompt_detector: Arc<PromptDetector>,
    activity_tx: watch::Sender<Instant>,
) -> Result<(), AppError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0_u8; 8192];
    loop {
        let bytes_read = reader.read(&mut buffer).await.map_err(AppError::Io)?;
        if bytes_read == 0 {
            break;
        }

        writer
            .write_all(&buffer[..bytes_read])
            .await
            .map_err(AppError::Io)?;
        writer.flush().await.map_err(AppError::Io)?;
        let _ = activity_tx.send(Instant::now());

        let event_type = match stream_name {
            "stderr" => EventType::StderrReceived,
            _ => EventType::OutputReceived,
        };
        let observed_sample = truncate_output_sample(&buffer[..bytes_read], max_sample_chars);
        let prompt = prompt_detector.detect(&observed_sample);
        let event_sample = if include_output_sample {
            Some(observed_sample.clone())
        } else {
            None
        };
        let event = build_event(
            &session_id,
            source.clone(),
            workspace.clone(),
            event_type,
            json_without_nulls(json!({
                "stream": stream_name,
                "bytes": bytes_read,
                "sample": event_sample,
            })),
        );
        warn_if_event_failed(client.post_event(&event).await, stream_name);

        if let Some(prompt) = prompt {
            let event = build_event(
                &session_id,
                source.clone(),
                workspace.clone(),
                EventType::PromptDetected,
                json!({
                    "stream": stream_name,
                    "pattern": prompt,
                }),
            );
            warn_if_event_failed(client.post_event(&event).await, "prompt.detected");
        }
    }

    Ok(())
}

async fn monitor_timeouts(
    client: DaemonClient,
    session_id: String,
    source: Source,
    workspace: String,
    silent_threshold_seconds: u64,
    stuck_threshold_seconds: u64,
    mut activity_rx: watch::Receiver<Instant>,
) {
    let silent_threshold = Duration::from_secs(silent_threshold_seconds.max(1));
    let stuck_threshold = Duration::from_secs(stuck_threshold_seconds.max(silent_threshold_seconds.max(1) + 1));
    let mut sent_silent = false;
    let mut sent_stuck = false;

    loop {
        tokio::select! {
            changed = activity_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                sent_silent = false;
                sent_stuck = false;
            }
            _ = sleep(Duration::from_millis(250)) => {
                let elapsed = activity_rx.borrow().elapsed();
                if !sent_silent && elapsed >= silent_threshold {
                    let event = build_event(
                        &session_id,
                        source.clone(),
                        workspace.clone(),
                        EventType::IdleTimeout,
                        json!({
                            "idle_ms": elapsed.as_millis(),
                            "threshold_ms": silent_threshold.as_millis(),
                        }),
                    );
                    warn_if_event_failed(client.post_event(&event).await, "idle.timeout");
                    sent_silent = true;
                }

                if !sent_stuck && elapsed >= stuck_threshold {
                    let event = build_event(
                        &session_id,
                        source.clone(),
                        workspace.clone(),
                        EventType::StuckTimeout,
                        json!({
                            "idle_ms": elapsed.as_millis(),
                            "threshold_ms": stuck_threshold.as_millis(),
                        }),
                    );
                    warn_if_event_failed(client.post_event(&event).await, "stuck.timeout");
                    sent_stuck = true;
                }
            }
        }
    }
}

async fn join_stream_task(
    task: tokio::task::JoinHandle<Result<(), AppError>>,
) -> Result<(), AppError> {
    task.await.map_err(AppError::Join)??;
    Ok(())
}

async fn ensure_daemon_running(config: &Config) -> Result<(), AppError> {
    if daemon_health(&config.daemon.host, config.daemon.port).await.is_ok() {
        return Ok(());
    }

    start_daemon()?;

    for _ in 0..40 {
        sleep(Duration::from_millis(250)).await;
        if daemon_health(&config.daemon.host, config.daemon.port).await.is_ok() {
            return Ok(());
        }
    }

    Err(AppError::DaemonUnavailable)
}

fn start_daemon() -> Result<(), AppError> {
    let daemon = find_daemon_binary().ok_or(AppError::DaemonBinaryNotFound)?;
    Command::new(daemon)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(AppError::Io)?;
    Ok(())
}

fn find_daemon_binary() -> Option<PathBuf> {
    if let Some(path) = env::var_os("AGENT_ORB_DAEMON") {
        return Some(PathBuf::from(path));
    }

    let exe_name = if cfg!(windows) {
        "agent_orbd.exe"
    } else {
        "agent_orbd"
    };
    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let sibling = dir.join(exe_name);
            if sibling.exists() {
                return Some(sibling);
            }
        }
    }

    Some(PathBuf::from(exe_name))
}

async fn daemon_health(host: &str, port: u16) -> Result<(), AppError> {
    let response =
        http_request(host, port, "GET", "/health", &[], Vec::new()).await?;

    if response.status_code == 200 {
        Ok(())
    } else {
        Err(AppError::HttpStatus(response.status_code))
    }
}

#[derive(Debug, Clone)]
struct DaemonClient {
    host: String,
    port: u16,
    token: String,
}

impl DaemonClient {
    fn new(host: String, port: u16, token: String) -> Self {
        Self { host, port, token }
    }

    async fn post_event(&self, event: &EventEnvelope) -> Result<(), AppError> {
        let body = serde_json::to_vec(event).map_err(AppError::Json)?;
        let auth = format!("Bearer {}", self.token);
        let headers = [
            ("Authorization", auth.as_str()),
            ("Content-Type", "application/json"),
        ];
        let response =
            http_request(&self.host, self.port, "POST", "/v1/events", &headers, body).await?;

        if (200..300).contains(&response.status_code) {
            Ok(())
        } else {
            Err(AppError::HttpStatus(response.status_code))
        }
    }
}

#[derive(Debug)]
struct HttpResponseHead {
    status_code: u16,
}

async fn http_request(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> Result<HttpResponseHead, AppError> {
    let mut stream = TcpStream::connect((host, port))
        .await
        .map_err(AppError::Io)?;
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");

    stream
        .write_all(request.as_bytes())
        .await
        .map_err(AppError::Io)?;
    if !body.is_empty() {
        stream.write_all(&body).await.map_err(AppError::Io)?;
    }
    stream.flush().await.map_err(AppError::Io)?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(AppError::Io)?;
    parse_http_response_head(&response)
}

fn parse_http_response_head(response: &[u8]) -> Result<HttpResponseHead, AppError> {
    let text = std::str::from_utf8(response).map_err(|_| AppError::InvalidHttpResponse)?;
    let status_line = text.lines().next().ok_or(AppError::InvalidHttpResponse)?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or(AppError::InvalidHttpResponse)?
        .parse::<u16>()
        .map_err(|_| AppError::InvalidHttpResponse)?;

    Ok(HttpResponseHead { status_code })
}

fn build_event(
    session_id: &str,
    source: Source,
    workspace: String,
    event_type: EventType,
    payload: serde_json::Value,
) -> EventEnvelope {
    EventEnvelope {
        version: "1.0".to_string(),
        event_id: Uuid::now_v7().to_string(),
        session_id: session_id.to_string(),
        source,
        workspace,
        event_type,
        timestamp: now_rfc3339(),
        payload,
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn detect_source(command: &str) -> Source {
    let file_name = Path::new(command)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .to_ascii_lowercase();

    if file_name.contains("codex") {
        Source::Codex
    } else if file_name.contains("claude") {
        Source::Claude
    } else {
        Source::Generic
    }
}

fn shell_join(command: &[String]) -> String {
    command
        .iter()
        .map(|part| {
            if part.chars().any(char::is_whitespace) {
                format!("\"{}\"", part.replace('"', "\\\""))
            } else {
                part.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn ensure_loopback_host(host: &str) -> Result<(), AppError> {
    let is_loopback = if host.eq_ignore_ascii_case("localhost") {
        true
    } else {
        host.parse::<IpAddr>().is_ok_and(|addr| addr.is_loopback())
    };

    if is_loopback {
        Ok(())
    } else {
        Err(AppError::UnsafeDaemonHost(host.to_string()))
    }
}

#[derive(Debug, Clone)]
struct PromptDetector {
    patterns: Vec<&'static str>,
}

impl PromptDetector {
    fn for_source(source: &Source) -> Self {
        let mut patterns = vec![
            "?",
            "confirm",
            "continue?",
            "yes/no",
            "approve",
            "permission",
            "press enter",
        ];

        match source {
            Source::Codex => patterns.extend(["approval", "allow", "deny"]),
            Source::Claude => patterns.extend(["do you want to proceed", "proceed?", "press enter"]),
            Source::Generic => {}
        }

        Self { patterns }
    }

    fn detect(&self, text: &str) -> Option<&'static str> {
        let lower = text.to_ascii_lowercase();
        self.patterns
            .iter()
            .copied()
            .find(|pattern| lower.contains(pattern))
    }
}

fn truncate_output_sample(bytes: &[u8], max_sample_chars: usize) -> String {
    let sample = String::from_utf8_lossy(bytes);
    truncate_chars(sample.as_ref(), max_sample_chars)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn json_without_nulls(mut value: serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Object(ref mut object) = value {
        object.retain(|_, value| !value.is_null());
    }
    value
}

fn default_config_dir() -> PathBuf {
    if let Some(dir) = env::var_os("AGENT_ORB_CONFIG_DIR") {
        return PathBuf::from(dir);
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("agent-orb");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("agent-orb");
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg_config_home) = env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg_config_home).join("agent-orb");
        }
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(".config").join("agent-orb");
        }
    }

    PathBuf::from(".").join("agent-orb")
}

fn read_token(config_dir: impl AsRef<Path>) -> Result<String, AppError> {
    let token_path = config_dir.as_ref().join(TOKEN_FILE_NAME);
    let token = fs::read_to_string(&token_path)
        .map_err(|source| AppError::ReadToken { token_path, source })?
        .trim()
        .to_string();

    if token.is_empty() {
        return Err(AppError::EmptyToken);
    }

    Ok(token)
}

fn warn_if_event_failed(result: Result<(), AppError>, event_name: &str) {
    if let Err(err) = result {
        eprintln!("agent_orb: failed to send {event_name}: {err}");
    }
}

#[derive(Debug)]
enum AppError {
    DaemonUnavailable,
    DaemonBinaryNotFound,
    EmptyCommand,
    EmptyToken,
    HttpStatus(u16),
    InvalidHttpResponse,
    Io(io::Error),
    Json(serde_json::Error),
    Join(JoinError),
    ReadToken {
        token_path: PathBuf,
        source: io::Error,
    },
    Spawn {
        command: String,
        source: io::Error,
    },
    UnsafeDaemonHost(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DaemonUnavailable => write!(
                f,
                "daemon is unavailable; start agent_orbd first or set AGENT_ORB_DAEMON"
            ),
            Self::DaemonBinaryNotFound => write!(
                f,
                "daemon is not running and agent_orbd could not be found; start agent_orbd first"
            ),
            Self::EmptyCommand => write!(f, "missing command after `agent_orb run --`"),
            Self::EmptyToken => write!(f, "local daemon token file is empty"),
            Self::HttpStatus(status) => write!(f, "daemon returned HTTP status {status}"),
            Self::InvalidHttpResponse => write!(f, "daemon returned an invalid HTTP response"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Json(err) => write!(f, "JSON error: {err}"),
            Self::Join(err) => write!(f, "stream task failed: {err}"),
            Self::ReadToken { token_path, source } => write!(
                f,
                "failed to read local daemon token at {}: {source}",
                token_path.display()
            ),
            Self::Spawn { command, source } => {
                write!(f, "failed to spawn target command `{command}`: {source}")
            }
            Self::UnsafeDaemonHost(host) => write!(
                f,
                "refusing to send events to non-loopback daemon host `{host}`"
            ),
        }
    }
}

impl std::error::Error for AppError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_known_sources_from_command_name() {
        assert_eq!(detect_source("codex"), Source::Codex);
        assert_eq!(detect_source("/usr/local/bin/claude"), Source::Claude);
        assert_eq!(detect_source("echo"), Source::Generic);
    }

    #[test]
    fn joins_command_for_payload() {
        let command = vec![
            "codex".to_string(),
            "-m".to_string(),
            "gpt-5 codex".to_string(),
        ];

        assert_eq!(shell_join(&command), "codex -m \"gpt-5 codex\"");
    }

    #[test]
    fn prompt_detector_finds_common_prompts() {
        let detector = PromptDetector::for_source(&Source::Codex);

        assert_eq!(
            detector.detect("Approve this command? [yes/no]"),
            Some("?")
        );
        assert_eq!(detector.detect("regular output"), None);
    }

    #[test]
    fn daemon_host_must_be_loopback() {
        assert!(ensure_loopback_host("127.0.0.1").is_ok());
        assert!(ensure_loopback_host("localhost").is_ok());
        assert!(ensure_loopback_host("0.0.0.0").is_err());
        assert!(ensure_loopback_host("192.168.1.10").is_err());
    }

    #[test]
    fn output_sample_is_bounded_and_prompt_detectable() {
        let detector = PromptDetector::for_source(&Source::Generic);

        assert_eq!(detector.detect(&truncate_output_sample(b"continue? yes/no", 512)), Some("?"));
        assert_eq!(truncate_output_sample(b"abcdef", 3), "abc");
    }

    #[test]
    fn parses_http_status_code() {
        let response = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}";

        let head = parse_http_response_head(response).expect("status should parse");

        assert_eq!(head.status_code, 200);
    }

    #[test]
    fn rejects_invalid_http_response() {
        assert!(parse_http_response_head(b"not-http").is_err());
    }
}
