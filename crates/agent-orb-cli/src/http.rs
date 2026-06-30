use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::error::AppError;

#[derive(Debug)]
pub struct HttpResponseHead {
    pub status_code: u16,
}

pub async fn http_request(
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
