#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{env, fmt, fs, io, net::IpAddr, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};
use tauri::Manager;

use agent_orb_core::config::{loopback_socket_addr, Config};

const TOKEN_FILE_NAME: &str = "token";
const COMPACT_MIN_SIZE: f64 = 32.0;
const PANEL_WIDTH: f64 = 360.0;
const PANEL_HEIGHT: f64 = 260.0;
const PANEL_MARGIN: f64 = 12.0;
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const HTTP_IO_TIMEOUT: Duration = Duration::from_secs(5);

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UiConfig {
    daemon: UiDaemonConfig,
    orb: UiOrbConfig,
    colors: UiColorConfig,
    behavior: UiBehaviorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UiDaemonConfig {
    host: String,
    port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UiOrbConfig {
    position: String,
    size: u16,
    opacity: f32,
    always_on_top: bool,
    click_through: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UiColorConfig {
    disconnected: String,
    idle: String,
    starting: String,
    active: String,
    thinking_like: String,
    waiting_input: String,
    compacting: String,
    completed: String,
    error: String,
    warning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UiBehaviorConfig {
    silent_threshold_seconds: u64,
    stuck_threshold_seconds: u64,
    completed_hold_seconds: u64,
}

#[tauri::command]
async fn get_status() -> Result<StatusSnapshot, String> {
    let config_dir = default_config_dir();
    let config = Config::load_from_dir_or_default(&config_dir);
    ensure_loopback_host(&config.daemon.host)?;
    let token = read_token(&config_dir).map_err(|err| err.to_string())?;
    let response = http_request(
        &config.daemon.host,
        config.daemon.port,
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
    let config_dir = default_config_dir();
    let config = Config::load_from_dir_or_default(&config_dir);
    ensure_loopback_host(&config.daemon.host)?;
    let token = read_token(&config_dir).map_err(|err| err.to_string())?;
    let response = http_request(
        &config.daemon.host,
        config.daemon.port,
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

#[tauri::command]
fn get_config() -> UiConfig {
    UiConfig::from(Config::load_from_dir_or_default(default_config_dir()))
}

#[tauri::command]
fn set_panel_open(window: tauri::WebviewWindow, open: bool) -> Result<(), String> {
    let config = Config::load_from_dir_or_default(default_config_dir());
    apply_panel_window_state(&window, open, &config).map_err(|err| err.to_string())
}

#[tauri::command]
fn start_drag(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|err| err.to_string())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let config = Config::load_from_dir_or_default(default_config_dir());
                let _ = window.set_always_on_top(config.orb.always_on_top);
                let _ = window.set_ignore_cursor_events(config.orb.click_through);
                let _ = apply_panel_window_state(&window, false, &config);
                let _ = position_window_from_config(&window, false, &config);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            clear_status,
            get_config,
            set_panel_open,
            start_drag
        ])
        .run(tauri::generate_context!())
        .expect("error while running Agent Orb UI");
}

fn apply_panel_window_state(
    window: &tauri::WebviewWindow,
    open: bool,
    config: &Config,
) -> tauri::Result<()> {
    let monitor = window.current_monitor()?.or(window.primary_monitor()?);
    let scale_factor = monitor
        .as_ref()
        .map(|monitor| monitor.scale_factor())
        .unwrap_or(1.0);
    let compact_size = f64::from(config.orb.size).max(COMPACT_MIN_SIZE);
    let (width, height) = if open {
        (PANEL_WIDTH, PANEL_HEIGHT)
    } else {
        (compact_size, compact_size)
    };
    let width_px = logical_to_physical_u32(width, scale_factor);
    let height_px = logical_to_physical_u32(height, scale_factor);

    window.set_size(tauri::PhysicalSize::new(width_px, height_px))?;
    let _ = window.set_always_on_top(config.orb.always_on_top);
    if open {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }

    Ok(())
}

fn position_window_from_config(
    window: &tauri::WebviewWindow,
    open: bool,
    config: &Config,
) -> tauri::Result<()> {
    let monitor = window.current_monitor()?.or(window.primary_monitor()?);
    let scale_factor = monitor
        .as_ref()
        .map(|monitor| monitor.scale_factor())
        .unwrap_or(1.0);
    let compact_size = f64::from(config.orb.size).max(COMPACT_MIN_SIZE);
    let (width, height) = if open {
        (PANEL_WIDTH, PANEL_HEIGHT)
    } else {
        (compact_size, compact_size)
    };
    let width_px = logical_to_physical_u32(width, scale_factor);
    let height_px = logical_to_physical_u32(height, scale_factor);
    if let Some(monitor) = monitor {
        let work_area = monitor.work_area();
        let margin = logical_to_physical_i32(PANEL_MARGIN, scale_factor);
        let wants_right = config.orb.position.contains("right");
        let wants_bottom = config.orb.position.contains("bottom");
        let x = if wants_right {
            work_area.position.x + work_area.size.width as i32 - width_px as i32 - margin
        } else {
            work_area.position.x + margin
        };
        let y = if wants_bottom {
            work_area.position.y + work_area.size.height as i32 - height_px as i32 - margin
        } else {
            work_area.position.y + margin
        };

        window.set_position(tauri::PhysicalPosition::new(
            x.max(work_area.position.x),
            y.max(work_area.position.y),
        ))?;
    }

    Ok(())
}

fn logical_to_physical_u32(value: f64, scale_factor: f64) -> u32 {
    (value * scale_factor).round().max(1.0) as u32
}

fn logical_to_physical_i32(value: f64, scale_factor: f64) -> i32 {
    (value * scale_factor).round().max(0.0) as i32
}

#[derive(Debug)]
struct HttpResponse {
    status_code: u16,
    body: Vec<u8>,
}

async fn http_request(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: Vec<u8>,
) -> Result<HttpResponse, UiError> {
    http_request_with_timeouts(
        host,
        port,
        method,
        path,
        headers,
        body,
        HTTP_CONNECT_TIMEOUT,
        HTTP_IO_TIMEOUT,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn http_request_with_timeouts(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: Vec<u8>,
    connect_timeout: Duration,
    io_timeout: Duration,
) -> Result<HttpResponse, UiError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::time::timeout;

    let mut stream = timeout(connect_timeout, TcpStream::connect((host, port)))
        .await
        .map_err(|_| UiError::HttpTimeout("connect"))??;
    let host_header = http_host_header(host, port);
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host_header}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");

    timeout(io_timeout, stream.write_all(request.as_bytes()))
        .await
        .map_err(|_| UiError::HttpTimeout("write"))??;
    if !body.is_empty() {
        timeout(io_timeout, stream.write_all(&body))
            .await
            .map_err(|_| UiError::HttpTimeout("write"))??;
    }
    timeout(io_timeout, stream.flush())
        .await
        .map_err(|_| UiError::HttpTimeout("write"))??;

    let mut raw = Vec::new();
    timeout(io_timeout, stream.read_to_end(&mut raw))
        .await
        .map_err(|_| UiError::HttpTimeout("read"))??;
    parse_http_response(&raw)
}

fn http_host_header(host: &str, port: u16) -> String {
    if matches!(host.parse::<IpAddr>(), Ok(IpAddr::V6(_))) {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn ensure_loopback_host(host: &str) -> Result<(), String> {
    if loopback_socket_addr(host, 0).is_some() {
        Ok(())
    } else {
        Err(UiError::UnsafeDaemonHost(host.to_string()).to_string())
    }
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

fn read_token(config_dir: impl AsRef<std::path::Path>) -> Result<String, UiError> {
    let path = config_dir.as_ref().join(TOKEN_FILE_NAME);
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

impl From<Config> for UiConfig {
    fn from(config: Config) -> Self {
        Self {
            daemon: UiDaemonConfig {
                host: config.daemon.host,
                port: config.daemon.port,
            },
            orb: UiOrbConfig {
                position: config.orb.position,
                size: config.orb.size,
                opacity: config.orb.opacity,
                always_on_top: config.orb.always_on_top,
                click_through: config.orb.click_through,
            },
            colors: UiColorConfig {
                disconnected: config.colors.disconnected,
                idle: config.colors.idle,
                starting: config.colors.starting,
                active: config.colors.active,
                thinking_like: config.colors.thinking_like,
                waiting_input: config.colors.waiting_input,
                compacting: config.colors.compacting,
                completed: config.colors.completed,
                error: config.colors.error,
                warning: config.colors.warning,
            },
            behavior: UiBehaviorConfig {
                silent_threshold_seconds: config.behavior.silent_threshold_seconds,
                stuck_threshold_seconds: config.behavior.stuck_threshold_seconds,
                completed_hold_seconds: config.behavior.completed_hold_seconds,
            },
        }
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
    HttpTimeout(&'static str),
    InvalidHttpResponse,
    Io(io::Error),
    ReadToken { path: PathBuf, source: io::Error },
    UnsafeDaemonHost(String),
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyToken => write!(f, "local daemon token file is empty"),
            Self::HttpTimeout(operation) => write!(f, "daemon HTTP {operation} timed out"),
            Self::InvalidHttpResponse => write!(f, "daemon returned an invalid HTTP response"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::ReadToken { path, source } => {
                write!(f, "failed to read token at {}: {source}", path.display())
            }
            Self::UnsafeDaemonHost(host) => {
                write!(
                    f,
                    "refusing to send the daemon token to non-loopback host `{host}`"
                )
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
    use tokio::net::TcpListener;

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

    #[test]
    fn brackets_ipv6_host_header() {
        assert_eq!(http_host_header("::1", 17321), "[::1]:17321");
        assert_eq!(http_host_header("localhost", 17321), "localhost:17321");
    }

    #[test]
    fn accepts_localhost_and_rejects_external_daemon_hosts() {
        assert!(ensure_loopback_host("localhost").is_ok());
        assert!(ensure_loopback_host("127.0.0.1").is_ok());
        assert!(ensure_loopback_host("example.com").is_err());
    }

    #[tokio::test]
    async fn times_out_when_daemon_never_responds() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let port = listener
            .local_addr()
            .expect("test listener should have an address")
            .port();
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("connection should arrive");
            tokio::time::sleep(Duration::from_secs(1)).await;
        });

        let result = http_request_with_timeouts(
            "127.0.0.1",
            port,
            "GET",
            "/health",
            &[],
            Vec::new(),
            Duration::from_millis(50),
            Duration::from_millis(50),
        )
        .await;

        assert!(matches!(result, Err(UiError::HttpTimeout("read"))));
        server.abort();
    }
}
