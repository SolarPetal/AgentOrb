use std::{env, process::Stdio, sync::Arc, time::Duration};

use agent_orb_core::{config::Config, event::EventType, source::Source};
use serde_json::json;
use time::OffsetDateTime;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    process::Command,
    sync::watch,
    time::{sleep, Instant},
};
use uuid::Uuid;

use crate::{
    config::{default_config_dir, ensure_loopback_host, read_token},
    daemon::{ensure_daemon_running, DaemonClient},
    error::AppError,
    event::{build_event, json_without_nulls},
    prompt::{truncate_output_sample, PromptDetector},
    shell::shell_join,
    source::detect_source,
};

pub async fn run_wrapped_command(command: Vec<String>) -> Result<i32, AppError> {
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
