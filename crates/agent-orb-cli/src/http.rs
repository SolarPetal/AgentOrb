use std::{net::IpAddr, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};

use crate::error::AppError;

#[derive(Debug)]
pub struct HttpResponseHead {
    pub status_code: u16,
}

const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const HTTP_IO_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn http_request(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> Result<HttpResponseHead, AppError> {
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
    headers: &[(&str, &str)],
    body: Vec<u8>,
    connect_timeout: Duration,
    io_timeout: Duration,
) -> Result<HttpResponseHead, AppError> {
    let mut stream = timeout(connect_timeout, TcpStream::connect((host, port)))
        .await
        .map_err(|_| AppError::HttpTimeout("connect"))?
        .map_err(AppError::Io)?;
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
        .map_err(|_| AppError::HttpTimeout("write"))?
        .map_err(AppError::Io)?;
    if !body.is_empty() {
        timeout(io_timeout, stream.write_all(&body))
            .await
            .map_err(|_| AppError::HttpTimeout("write"))?
            .map_err(AppError::Io)?;
    }
    timeout(io_timeout, stream.flush())
        .await
        .map_err(|_| AppError::HttpTimeout("write"))?
        .map_err(AppError::Io)?;

    let mut response = Vec::new();
    timeout(io_timeout, stream.read_to_end(&mut response))
        .await
        .map_err(|_| AppError::HttpTimeout("read"))?
        .map_err(AppError::Io)?;
    parse_http_response_head(&response)
}

fn http_host_header(host: &str, port: u16) -> String {
    if matches!(host.parse::<IpAddr>(), Ok(IpAddr::V6(_))) {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

pub fn parse_http_response_head(response: &[u8]) -> Result<HttpResponseHead, AppError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

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

    #[test]
    fn brackets_ipv6_host_header() {
        assert_eq!(http_host_header("::1", 17321), "[::1]:17321");
        assert_eq!(http_host_header("127.0.0.1", 17321), "127.0.0.1:17321");
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

        assert!(matches!(result, Err(AppError::HttpTimeout("read"))));
        server.abort();
    }
}
