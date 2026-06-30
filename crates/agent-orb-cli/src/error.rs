use std::{fmt, io, path::PathBuf};

use tokio::task::JoinError;

#[derive(Debug)]
pub enum AppError {
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
