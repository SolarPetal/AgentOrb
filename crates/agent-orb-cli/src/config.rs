use std::{
    env, fs,
    net::IpAddr,
    path::{Path, PathBuf},
};

use crate::error::AppError;

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

pub fn read_token(config_dir: impl AsRef<Path>) -> Result<String, AppError> {
    let token_path = config_dir.as_ref().join(TOKEN_FILE_NAME);
    let token = fs::read_to_string(&token_path)
        .map_err(|source| AppError::ReadToken { token_path, source })?
        .trim()
        .to_string();

    if token.is_empty() {
        return Err(AppError::EmptyToken);
    }

    Ok(token)
}

pub fn ensure_loopback_host(host: &str) -> Result<(), AppError> {
    let is_loopback = if host.eq_ignore_ascii_case("localhost") {
        true
    } else {
        host.parse::<IpAddr>().is_ok_and(|addr| addr.is_loopback())
    };

    if is_loopback {
        Ok(())
    } else {
        Err(AppError::UnsafeDaemonHost(host.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_host_must_be_loopback() {
        assert!(ensure_loopback_host("127.0.0.1").is_ok());
        assert!(ensure_loopback_host("localhost").is_ok());
        assert!(ensure_loopback_host("0.0.0.0").is_err());
        assert!(ensure_loopback_host("192.168.1.10").is_err());
    }
}
