#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{env, fmt, fs, io, path::PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Manager;

const DAEMON_HOST: &str = "127.0.0.1";
const DAEMON_PORT: u16 = 17321;
const TOKEN_FILE_NAME: &str = "token";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusSnapshot {
    status: String,
    visual: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
    message: String,
}

#[tauri::command]
async fn get_status() -> Result<StatusSnapshot, String> {
    let token = read_token().map_err(|err| err.to_string())?;
    let response = http_request(
        "GET",
        "/v1/status",
        &[("Authorization", format!("Bearer {token}"))],
        Vec::new(),
    )
    .await
    .map_err(|err| err.to_string())?;

    if response.status_code != 200 {
        return Err(format!("daemon returned HTTP {}", response.status_code));
    }

    serde_json::from_slice(&response.body).map_err(|err| err.to_string())
}

#[tauri::command]
async fn clear_status() -> Result<(), String> {
    let token = read_token().map_err(|err| err.to_string())?;
    let response = http_request(
        "POST",
        "/v1/status/clear",
        &[("Authorization", format!("Bearer {token}"))],
        Vec::new(),
    )
    .await
    .map_err(|err| err.to_string())?;

    if (200..300).contains(&response.status_code) {
        Ok(())
    } else {
        Err(format!("daemon returned HTTP {}", response.status_code))
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_always_on_top(true);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_status, clear_status])
        .run(tauri::generate_context!())
        .expect("error while running Agent Orb UI");
}

#[derive(Debug)]
struct HttpResponse {
    status_code: u16,
    body: Vec<u8>,
}

async fn http_request(
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: Vec<u8>,
) -> Result<HttpResponse, UiError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect((DAEMON_HOST, DAEMON_PORT)).await?;
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {DAEMON_HOST}:{DAEMON_PORT}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");

    stream.write_all(request.as_bytes()).await?;
    if !body.is_empty() {
        stream.write_all(&body).await?;
    }
    stream.flush().await?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    parse_http_response(&raw)
}

fn parse_http_response(raw: &[u8]) -> Result<HttpResponse, UiError> {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(UiError::InvalidHttpResponse)?;
    let (head, body) = raw.split_at(split + 4);
    let head = std::str::from_utf8(head).map_err(|_| UiError::InvalidHttpResponse)?;
    let status_code = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or(UiError::InvalidHttpResponse)?;

    Ok(HttpResponse {
        status_code,
        body: body.to_vec(),
    })
}

fn read_token() -> Result<String, UiError> {
    let path = default_config_dir().join(TOKEN_FILE_NAME);
    let token = fs::read_to_string(&path)
        .map_err(|source| UiError::ReadToken { path, source })?
        .trim()
        .to_string();

    if token.is_empty() {
        Err(UiError::EmptyToken)
    } else {
        Ok(token)
    }
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

#[derive(Debug)]
enum UiError {
    EmptyToken,
    InvalidHttpResponse,
    Io(io::Error),
    ReadToken { path: PathBuf, source: io::Error },
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyToken => write!(f, "local daemon token file is empty"),
            Self::InvalidHttpResponse => write!(f, "daemon returned an invalid HTTP response"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::ReadToken { path, source } => {
                write!(f, "failed to read token at {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for UiError {}

impl From<io::Error> for UiError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_response_body() {
        let raw = b"HTTP/1.1 200 OK\r\ncontent-length: 15\r\n\r\n{\"status\":true}";
        let response = parse_http_response(raw).expect("response should parse");

        assert_eq!(response.status_code, 200);
        assert_eq!(response.body, br#"{"status":true}"#);
    }

    #[test]
    fn rejects_invalid_http_response() {
        assert!(parse_http_response(b"not-http").is_err());
    }
}
