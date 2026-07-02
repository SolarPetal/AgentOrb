use std::{
    env,
    io::{Read, Write},
    process::Stdio,
    sync::Arc,
    thread,
    time::Duration,
};

use agent_orb_core::{
    config::Config,
    event::{EventEnvelope, EventType},
    source::Source,
};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::json;
use time::OffsetDateTime;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    process::Command,
    sync::{mpsc, watch},
    time::{sleep, Instant},
};
use uuid::Uuid;

use crate::{
    config::{default_config_dir, ensure_loopback_host, read_token},
    daemon::{ensure_daemon_running, DaemonClient},
    error::AppError,
    event::{build_event, json_without_nulls},
    prompt::{truncate_output_sample, PromptDetector, StatusDetector},
    shell::shell_join,
    source::detect_source,
};

type EventQueueSender = mpsc::UnboundedSender<(EventEnvelope, &'static str)>;

pub async fn run_wrapped_command(command: Vec<String>) -> Result<i32, AppError> {
    if command.is_empty() {
        return Err(AppError::EmptyCommand);
    }

    let config_dir = default_config_dir();
    let config = Config::load_from_dir_or_default(&config_dir);
    ensure_loopback_host(&config.daemon.host)?;
    ensure_daemon_running(&config, &config_dir).await?;
    let token = read_token(&config_dir)?;
    let client = DaemonClient::new(config.daemon.host.clone(), config.daemon.port, token);

    let source = detect_source(&command[0]);
    let workspace = env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| ".".to_string());
    let session_id = Uuid::now_v7().to_string();
    let started_at = OffsetDateTime::now_utc();

    if should_use_observed_terminal(&source) {
        return run_pty_observed_command(
            &command, config, client, source, workspace, session_id, started_at,
        )
        .await;
    }

    if should_use_tty_passthrough(&source) {
        return run_tty_passthrough_command(
            &command, config, client, source, workspace, session_id, started_at,
        )
        .await;
    }

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
    let status_detector = Arc::new(StatusDetector::for_source(&source));
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
            status_detector.clone(),
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
            status_detector.clone(),
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

async fn run_pty_observed_command(
    command: &[String],
    config: Config,
    client: DaemonClient,
    source: Source,
    workspace: String,
    session_id: String,
    started_at: OffsetDateTime,
) -> Result<i32, AppError> {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<(EventEnvelope, &'static str)>();
    let event_client = client.clone();
    let event_task = tokio::spawn(async move {
        while let Some((event, event_name)) = event_rx.recv().await {
            warn_if_event_failed(event_client.post_event(&event).await, event_name);
        }
    });

    let (activity_tx, activity_rx) = watch::channel(Instant::now());
    let timeout_task = tokio::spawn(monitor_timeouts(
        client,
        session_id.clone(),
        source.clone(),
        workspace.clone(),
        config.behavior.silent_threshold_seconds,
        config.behavior.stuck_threshold_seconds,
        activity_rx,
    ));

    let command = command.to_vec();
    let include_output_sample = config.privacy.include_output_sample;
    let max_sample_chars = config.privacy.max_sample_chars;
    let exit_code = tokio::task::spawn_blocking(move || {
        run_pty_observed_command_blocking(
            command,
            session_id,
            source,
            workspace,
            started_at,
            include_output_sample,
            max_sample_chars,
            event_tx,
            activity_tx,
        )
    })
    .await
    .map_err(AppError::Join)??;

    timeout_task.await.map_err(AppError::Join)?;
    event_task.await.map_err(AppError::Join)?;

    Ok(exit_code)
}

#[allow(clippy::too_many_arguments)]
fn run_pty_observed_command_blocking(
    command: Vec<String>,
    session_id: String,
    source: Source,
    workspace: String,
    started_at: OffsetDateTime,
    include_output_sample: bool,
    max_sample_chars: usize,
    event_tx: EventQueueSender,
    activity_tx: watch::Sender<Instant>,
) -> Result<i32, AppError> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(detect_terminal_size())
        .map_err(|err| AppError::Terminal(err.to_string()))?;
    let portable_pty::PtyPair { master, slave } = pair;

    let mut builder = CommandBuilder::new(&command[0]);
    builder.args(&command[1..]);
    if let Ok(cwd) = env::current_dir() {
        builder.cwd(cwd.as_os_str());
    }

    let mut child = slave
        .spawn_command(builder)
        .map_err(|err| AppError::SpawnTerminal {
            command: command[0].clone(),
            source: err.to_string(),
        })?;
    drop(slave);

    queue_event(
        &event_tx,
        build_event(
            &session_id,
            source.clone(),
            workspace.clone(),
            EventType::SessionStarted,
            json_without_nulls(json!({
                "command": shell_join(&command),
                "pid": child.process_id(),
                "platform": env::consts::OS,
                "stdio": "pty",
            })),
        ),
        "session.started",
    );

    queue_event(
        &event_tx,
        build_event(
            &session_id,
            source.clone(),
            workspace.clone(),
            EventType::StatusHint,
            json!({
                "status": "thinking",
                "reason": "adapter started in observed pty",
            }),
        ),
        "status.hint",
    );

    let reader = master
        .try_clone_reader()
        .map_err(|err| AppError::Terminal(err.to_string()))?;
    let writer = master
        .take_writer()
        .map_err(|err| AppError::Terminal(err.to_string()))?;

    let output_thread = {
        let event_tx = event_tx.clone();
        let activity_tx = activity_tx.clone();
        let prompt_detector = PromptDetector::for_source(&source);
        let status_detector = StatusDetector::for_source(&source);
        let session_id = session_id.clone();
        let source = source.clone();
        let workspace = workspace.clone();

        thread::spawn(move || {
            forward_pty_output_blocking(
                reader,
                event_tx,
                activity_tx,
                prompt_detector,
                status_detector,
                session_id,
                source,
                workspace,
                include_output_sample,
                max_sample_chars,
            )
        })
    };

    let _raw_mode = RawModeGuard::enable();
    thread::spawn(move || forward_stdin_to_pty_blocking(writer));

    let status = child.wait().map_err(AppError::Io)?;
    let exit_code = status.exit_code() as i32;

    let _ = output_thread.join();
    drop(activity_tx);

    let duration_ms = (OffsetDateTime::now_utc() - started_at).whole_milliseconds();
    queue_event(
        &event_tx,
        build_event(
            &session_id,
            source,
            workspace,
            EventType::ProcessExited,
            json!({
                "exit_code": exit_code,
                "duration_ms": duration_ms.max(0),
                "stdio": "pty",
            }),
        ),
        "process.exited",
    );

    Ok(exit_code)
}

async fn run_tty_passthrough_command(
    command: &[String],
    config: Config,
    client: DaemonClient,
    source: Source,
    workspace: String,
    session_id: String,
    started_at: OffsetDateTime,
) -> Result<i32, AppError> {
    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
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
            "command": shell_join(command),
            "pid": pid,
            "platform": env::consts::OS,
            "stdio": "inherit",
        }),
    );
    warn_if_event_failed(client.post_event(&started_event).await, "session.started");

    let active_event = build_event(
        &session_id,
        source.clone(),
        workspace.clone(),
        EventType::OutputReceived,
        json!({
            "stream": "tty",
            "bytes": 0,
            "stdio": "inherit",
            "synthetic": true,
        }),
    );
    warn_if_event_failed(client.post_event(&active_event).await, "tty.active");

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

    let status = child.wait().await.map_err(AppError::Io)?;
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

fn should_use_tty_passthrough(source: &Source) -> bool {
    matches!(source, Source::Codex | Source::Claude)
}

fn should_use_observed_terminal(source: &Source) -> bool {
    adapter_requires_observed_terminal(source) && !observed_terminal_disabled()
}

fn adapter_requires_observed_terminal(source: &Source) -> bool {
    matches!(source, Source::Codex | Source::Claude)
}

fn observed_terminal_disabled() -> bool {
    env::var("AGENT_ORB_DISABLE_PTY").is_ok_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[allow(clippy::too_many_arguments)]
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
    status_detector: Arc<StatusDetector>,
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

        if let Some(status_hint) = status_detector.detect(&observed_sample) {
            let event = build_event(
                &session_id,
                source.clone(),
                workspace.clone(),
                EventType::StatusHint,
                json!({
                    "stream": stream_name,
                    "status": status_hint.as_str(),
                }),
            );
            warn_if_event_failed(client.post_event(&event).await, "status.hint");
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn forward_pty_output_blocking(
    mut reader: Box<dyn Read + Send>,
    event_tx: EventQueueSender,
    activity_tx: watch::Sender<Instant>,
    prompt_detector: PromptDetector,
    status_detector: StatusDetector,
    session_id: String,
    source: Source,
    workspace: String,
    include_output_sample: bool,
    max_sample_chars: usize,
) {
    let mut stdout = std::io::stdout();
    let mut buffer = vec![0_u8; 8192];

    loop {
        let bytes_read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => bytes_read,
            Err(_) => break,
        };

        let _ = stdout.write_all(&buffer[..bytes_read]);
        let _ = stdout.flush();
        let _ = activity_tx.send(Instant::now());

        let observed_sample = truncate_output_sample(&buffer[..bytes_read], max_sample_chars);
        let prompt = prompt_detector.detect(&observed_sample);
        let status_hint = status_detector.detect(&observed_sample);
        let event_sample = if include_output_sample {
            Some(observed_sample.clone())
        } else {
            None
        };

        queue_event(
            &event_tx,
            build_event(
                &session_id,
                source.clone(),
                workspace.clone(),
                EventType::OutputReceived,
                json_without_nulls(json!({
                    "stream": "pty",
                    "bytes": bytes_read,
                    "sample": event_sample,
                })),
            ),
            "pty.output",
        );

        if let Some(prompt) = prompt {
            queue_event(
                &event_tx,
                build_event(
                    &session_id,
                    source.clone(),
                    workspace.clone(),
                    EventType::PromptDetected,
                    json!({
                        "stream": "pty",
                        "pattern": prompt,
                    }),
                ),
                "prompt.detected",
            );
        }

        if let Some(status_hint) = status_hint {
            queue_event(
                &event_tx,
                build_event(
                    &session_id,
                    source.clone(),
                    workspace.clone(),
                    EventType::StatusHint,
                    json!({
                        "stream": "pty",
                        "status": status_hint.as_str(),
                    }),
                ),
                "status.hint",
            );
        }
    }
}

fn forward_stdin_to_pty_blocking(mut writer: Box<dyn Write + Send>) {
    let mut stdin = std::io::stdin();
    let mut buffer = vec![0_u8; 8192];

    loop {
        let bytes_read = match stdin.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => bytes_read,
            Err(_) => break,
        };

        if writer.write_all(&buffer[..bytes_read]).is_err() {
            break;
        }
        let _ = writer.flush();
    }
}

fn detect_terminal_size() -> PtySize {
    PtySize {
        rows: env::var("LINES")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|rows| *rows > 0)
            .unwrap_or(30),
        cols: env::var("COLUMNS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|cols| *cols > 0)
            .unwrap_or(120),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn queue_event(event_tx: &EventQueueSender, event: EventEnvelope, event_name: &'static str) {
    let _ = event_tx.send((event, event_name));
}

struct RawModeGuard {
    enabled: bool,
}

impl RawModeGuard {
    fn enable() -> Self {
        Self {
            enabled: enable_raw_mode().is_ok(),
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.enabled {
            let _ = disable_raw_mode();
        }
    }
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
    let stuck_threshold =
        Duration::from_secs(stuck_threshold_seconds.max(silent_threshold_seconds.max(1) + 1));
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

fn warn_if_event_failed(result: Result<(), AppError>, event_name: &str) {
    if let Err(err) = result {
        eprintln!("agent_orb: failed to send {event_name}: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::{adapter_requires_observed_terminal, should_use_tty_passthrough};
    use agent_orb_core::source::Source;

    #[test]
    fn codex_and_claude_need_terminal_observation() {
        assert!(adapter_requires_observed_terminal(&Source::Codex));
        assert!(adapter_requires_observed_terminal(&Source::Claude));
        assert!(!adapter_requires_observed_terminal(&Source::Generic));
    }

    #[test]
    fn legacy_tty_passthrough_still_exists_as_escape_hatch() {
        assert!(should_use_tty_passthrough(&Source::Codex));
        assert!(should_use_tty_passthrough(&Source::Claude));
        assert!(!should_use_tty_passthrough(&Source::Generic));
    }
}
