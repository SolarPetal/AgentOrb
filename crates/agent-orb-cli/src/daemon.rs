use std::{
    env,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use agent_orb_core::{config::Config, event::EventEnvelope};
use tokio::{process::Command, time::sleep};

use crate::{config::read_token, error::AppError, http::http_request};

#[derive(Debug, Clone)]
pub struct DaemonClient {
    host: String,
    port: u16,
    token: String,
}

impl DaemonClient {
    pub fn new(host: String, port: u16, token: String) -> Self {
        Self { host, port, token }
    }

    pub async fn post_event(&self, event: &EventEnvelope) -> Result<(), AppError> {
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

pub async fn ensure_daemon_running(
    config: &Config,
    config_dir: impl AsRef<Path>,
) -> Result<(), AppError> {
    let config_dir = config_dir.as_ref();
    if authenticated_status(&config.daemon.host, config.daemon.port, config_dir)
        .await
        .is_ok()
    {
        return Ok(());
    }

    if daemon_health(&config.daemon.host, config.daemon.port)
        .await
        .is_ok()
    {
        return Err(AppError::DaemonTokenMismatch {
            host: config.daemon.host.clone(),
            port: config.daemon.port,
            token_path: config_dir.join("token"),
        });
    }

    if !config.daemon.auto_start {
        return Err(AppError::DaemonAutoStartDisabled {
            host: config.daemon.host.clone(),
            port: config.daemon.port,
        });
    }

    start_daemon()?;

    for _ in 0..40 {
        sleep(Duration::from_millis(250)).await;
        if authenticated_status(&config.daemon.host, config.daemon.port, config_dir)
            .await
            .is_ok()
        {
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
    let response = http_request(host, port, "GET", "/health", &[], Vec::new()).await?;

    if response.status_code == 200 {
        Ok(())
    } else {
        Err(AppError::HttpStatus(response.status_code))
    }
}

async fn authenticated_status(host: &str, port: u16, config_dir: &Path) -> Result<(), AppError> {
    let token = read_token(config_dir)?;
    let auth = format!("Bearer {token}");
    let headers = [("Authorization", auth.as_str())];
    let response = http_request(host, port, "GET", "/v1/status", &headers, Vec::new()).await?;

    if response.status_code == 200 {
        Ok(())
    } else {
        Err(AppError::HttpStatus(response.status_code))
    }
}
