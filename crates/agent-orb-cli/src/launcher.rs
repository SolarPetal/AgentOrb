use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    time::Duration,
};

use agent_orb_core::config::Config;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::{
    process::{Child, Command},
    time::{sleep, timeout},
};

use crate::{
    config::{default_config_dir, ensure_loopback_host},
    daemon::ensure_daemon_running_with_lifecycle,
    error::AppError,
    runner::run_wrapped_command,
};

const LAUNCHER_LOG_FILE: &str = "agent-orb-launcher.log";

pub async fn launch_adapter(adapter: String, args: Vec<String>) -> Result<i32, AppError> {
    if adapter.trim().is_empty() {
        return Err(AppError::EmptyCommand);
    }

    let config_dir = default_config_dir();
    let log_path = launcher_log_path(&config_dir);
    log_line(
        &log_path,
        format!("launch requested adapter={adapter:?} args={args:?}"),
    );

    let config = Config::load_from_dir_or_default(&config_dir);
    ensure_loopback_host(&config.daemon.host)?;

    let mut daemon_lifecycle = ensure_daemon_running_with_lifecycle(&config, &config_dir).await?;
    if let Some(child) = daemon_lifecycle.started_child.as_ref() {
        log_line(
            &log_path,
            format!("started session-local daemon pid={:?}", child.id()),
        );
    } else {
        log_line(&log_path, "using existing daemon");
    }

    let mut ui_child = match start_ui_if_needed(&log_path).await {
        Ok(child) => child,
        Err(err) => {
            log_line(&log_path, format!("failed to start orb UI: {err}"));
            eprintln!("agent_orb: orb UI did not start; continuing without desktop orb ({err})");
            None
        }
    };

    let mut command = Vec::with_capacity(1 + args.len());
    command.push(adapter);
    command.extend(args);
    let result = run_wrapped_command(command).await;

    if let Some(child) = ui_child.as_mut() {
        stop_child("orb UI", child, &log_path).await;
    }
    if let Some(child) = daemon_lifecycle.started_child.as_mut() {
        stop_child("daemon", child, &log_path).await;
    }

    match &result {
        Ok(code) => log_line(&log_path, format!("launch completed exit_code={code}")),
        Err(err) => log_line(&log_path, format!("launch failed: {err}")),
    }

    result
}

async fn start_ui_if_needed(log_path: &Path) -> Result<Option<Child>, AppError> {
    if !desktop_session_available() {
        log_line(log_path, "desktop session not detected; skipping orb UI");
        return Ok(None);
    }

    let process_name = ui_process_name();
    if is_process_running(process_name) {
        log_line(log_path, format!("orb UI already running: {process_name}"));
        return Ok(None);
    }

    let ui = find_ui_binary();
    let Some(ui) = ui else {
        log_line(log_path, "orb UI binary not found");
        return Ok(None);
    };

    let mut command = Command::new(&ui);
    command.stdin(Stdio::null());
    attach_log_stdio(&mut command, log_path);

    let mut child = command.spawn().map_err(|source| AppError::Spawn {
        command: ui.display().to_string(),
        source,
    })?;
    log_line(
        log_path,
        format!("started session-local orb UI pid={:?}", child.id()),
    );

    sleep(Duration::from_millis(500)).await;
    if let Some(status) = child.try_wait().map_err(AppError::Io)? {
        log_line(
            log_path,
            format!("orb UI exited immediately with status={status}"),
        );
        return Ok(None);
    }

    Ok(Some(child))
}

fn desktop_session_available() -> bool {
    if cfg!(windows) || cfg!(target_os = "macos") {
        return true;
    }

    env::var_os("DISPLAY").is_some() || env::var_os("WAYLAND_DISPLAY").is_some()
}

fn find_ui_binary() -> Option<PathBuf> {
    if let Some(path) = env::var_os("AGENT_ORB_UI") {
        return Some(PathBuf::from(path));
    }

    let exe_name = ui_process_name();
    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let sibling = dir.join(exe_name);
            if sibling.exists() {
                return Some(sibling);
            }
        }
    }

    command_on_path(exe_name).or_else(|| Some(PathBuf::from(exe_name)))
}

fn ui_process_name() -> &'static str {
    if cfg!(windows) {
        "agent-orb-ui.exe"
    } else {
        "agent-orb-ui"
    }
}

fn command_on_path(command: &str) -> Option<PathBuf> {
    let path_env = env::var_os("PATH")?;
    let candidates: Vec<String> = if cfg!(windows) {
        let path_ext = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        std::iter::once(command.to_string())
            .chain(
                path_ext
                    .split(';')
                    .filter(|ext| !ext.is_empty())
                    .map(|ext| format!("{command}{ext}")),
            )
            .collect()
    } else {
        vec![command.to_string()]
    };

    env::split_paths(&path_env)
        .flat_map(|dir| candidates.iter().map(move |candidate| dir.join(candidate)))
        .find(|candidate| candidate.exists())
}

fn is_process_running(process_name: &str) -> bool {
    if cfg!(windows) {
        let filter = format!("IMAGENAME eq {process_name}");
        return StdCommand::new("tasklist")
            .args(["/FI", filter.as_str()])
            .output()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .to_ascii_lowercase()
                    .contains(&process_name.to_ascii_lowercase())
            })
            .unwrap_or(false);
    }

    StdCommand::new("pgrep")
        .args(["-x", process_name])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn attach_log_stdio(command: &mut Command, log_path: &Path) {
    let stdout = open_append_log(log_path).map(Stdio::from);
    let stderr = open_append_log(log_path).map(Stdio::from);

    command.stdout(stdout.unwrap_or_else(Stdio::null));
    command.stderr(stderr.unwrap_or_else(Stdio::null));
}

fn open_append_log(log_path: &Path) -> Option<fs::File> {
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .ok()
}

async fn stop_child(label: &str, child: &mut Child, log_path: &Path) {
    match child.try_wait() {
        Ok(Some(status)) => {
            log_line(log_path, format!("{label} already exited status={status}"));
            return;
        }
        Ok(None) => {}
        Err(err) => {
            log_line(log_path, format!("{label} status check failed: {err}"));
        }
    }

    log_line(log_path, format!("stopping session-local {label}"));
    if let Err(err) = child.start_kill() {
        log_line(log_path, format!("failed to request {label} stop: {err}"));
        return;
    }

    match timeout(Duration::from_secs(2), child.wait()).await {
        Ok(Ok(status)) => log_line(log_path, format!("{label} stopped status={status}")),
        Ok(Err(err)) => log_line(log_path, format!("{label} wait failed: {err}")),
        Err(_) => log_line(log_path, format!("{label} did not exit within timeout")),
    }
}

fn launcher_log_path(config_dir: &Path) -> PathBuf {
    config_dir.join("logs").join(LAUNCHER_LOG_FILE)
}

fn log_line(log_path: &Path, message: impl AsRef<str>) {
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let timestamp = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown-time".to_string());

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "[{timestamp}] {}", message.as_ref());
    }
}
