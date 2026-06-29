use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use axum::http::{header::AUTHORIZATION, HeaderMap};
use uuid::Uuid;

const TOKEN_FILE_NAME: &str = "token";

pub fn default_config_dir() -> PathBuf {
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

pub fn token_path(config_dir: impl AsRef<Path>) -> PathBuf {
    config_dir.as_ref().join(TOKEN_FILE_NAME)
}

pub fn load_or_create_token(config_dir: impl AsRef<Path>) -> io::Result<String> {
    let config_dir = config_dir.as_ref();
    fs::create_dir_all(config_dir)?;

    let path = token_path(config_dir);
    if path.exists() {
        let token = fs::read_to_string(&path)?.trim().to_string();
        if !token.is_empty() {
            ensure_private_permissions(&path)?;
            return Ok(token);
        }
    }

    let token = generate_token();
    write_token_file(&path, &token)?;
    Ok(token)
}

pub fn is_authorized(headers: &HeaderMap, token: &str) -> bool {
    let expected = format!("Bearer {token}");
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|actual| actual == expected)
}

fn generate_token() -> String {
    format!("agent-orb-{}", Uuid::now_v7().as_simple())
}

fn write_token_file(path: &Path, token: &str) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(token.as_bytes())?;
        file.write_all(b"\n")?;
        ensure_private_permissions(path)?;
    }

    #[cfg(not(unix))]
    {
        fs::write(path, format!("{token}\n"))?;
    }

    Ok(())
}

fn ensure_private_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        env::temp_dir().join(format!(
            "agent-orb-security-test-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn token_is_generated_and_reused() {
        let dir = temp_config_dir();

        let first = load_or_create_token(&dir).expect("token should be created");
        let second = load_or_create_token(&dir).expect("token should be reused");

        assert_eq!(first, second);
        assert!(first.starts_with("agent-orb-"));
        assert!(token_path(&dir).exists());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn authorization_requires_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer expected-token"),
        );

        assert!(is_authorized(&headers, "expected-token"));
        assert!(!is_authorized(&headers, "other-token"));
        assert!(!is_authorized(&HeaderMap::new(), "expected-token"));
    }

    #[cfg(unix)]
    #[test]
    fn token_file_is_private_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_config_dir();
        let _ = load_or_create_token(&dir).expect("token should be created");
        let mode = fs::metadata(token_path(&dir))
            .expect("token metadata should exist")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(mode, 0o600);

        fs::remove_dir_all(dir).ok();
    }
}
