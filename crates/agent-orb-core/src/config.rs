use std::{
    fmt, fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
};

use serde::{Deserialize, Serialize};

pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Resolve a daemon host accepted by Agent Orb to a concrete loopback address.
///
/// `localhost` is normalized to IPv4 so every component binds and connects to
/// the same address instead of depending on platform-specific DNS ordering.
pub fn loopback_socket_addr(host: &str, port: u16) -> Option<SocketAddr> {
    if host.eq_ignore_ascii_case("localhost") {
        return Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port));
    }

    host.parse::<IpAddr>()
        .ok()
        .filter(IpAddr::is_loopback)
        .map(|ip| SocketAddr::new(ip, port))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub orb: OrbConfig,
    #[serde(default)]
    pub colors: ColorConfig,
    #[serde(default)]
    pub behavior: BehaviorConfig,
    #[serde(default)]
    pub privacy: PrivacyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "defaults::daemon_host")]
    pub host: String,
    #[serde(default = "defaults::daemon_port")]
    pub port: u16,
    #[serde(default = "defaults::daemon_auto_start")]
    pub auto_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrbConfig {
    #[serde(default = "defaults::orb_position")]
    pub position: String,
    #[serde(default = "defaults::orb_size")]
    pub size: u16,
    #[serde(default = "defaults::orb_opacity")]
    pub opacity: f32,
    #[serde(default = "defaults::orb_always_on_top")]
    pub always_on_top: bool,
    #[serde(default = "defaults::orb_click_through")]
    pub click_through: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConfig {
    #[serde(default = "defaults::color_disconnected")]
    pub disconnected: String,
    #[serde(default = "defaults::color_idle")]
    pub idle: String,
    #[serde(default = "defaults::color_starting")]
    pub starting: String,
    #[serde(default = "defaults::color_active")]
    pub active: String,
    #[serde(default = "defaults::color_thinking_like")]
    pub thinking_like: String,
    #[serde(default = "defaults::color_waiting_input")]
    pub waiting_input: String,
    #[serde(default = "defaults::color_compacting")]
    pub compacting: String,
    #[serde(default = "defaults::color_completed")]
    pub completed: String,
    #[serde(default = "defaults::color_error")]
    pub error: String,
    #[serde(default = "defaults::color_warning")]
    pub warning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    #[serde(default = "defaults::silent_threshold_seconds")]
    pub silent_threshold_seconds: u64,
    #[serde(default = "defaults::stuck_threshold_seconds")]
    pub stuck_threshold_seconds: u64,
    #[serde(default = "defaults::completed_hold_seconds")]
    pub completed_hold_seconds: u64,
    #[serde(default = "defaults::error_requires_click_to_clear")]
    pub error_requires_click_to_clear: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    #[serde(default = "defaults::include_output_sample")]
    pub include_output_sample: bool,
    #[serde(default = "defaults::max_sample_chars")]
    pub max_sample_chars: usize,
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(toml::de::Error),
}

impl Config {
    pub fn from_toml_str(input: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(strip_bom(input))
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let input = fs::read_to_string(path).map_err(ConfigError::Io)?;
        Self::from_toml_str(&input).map_err(ConfigError::Parse)
    }

    pub fn load_from_dir_or_default(config_dir: impl AsRef<Path>) -> Self {
        Self::load_from_path(config_dir.as_ref().join(CONFIG_FILE_NAME)).unwrap_or_default()
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            host: defaults::daemon_host(),
            port: defaults::daemon_port(),
            auto_start: defaults::daemon_auto_start(),
        }
    }
}

impl Default for OrbConfig {
    fn default() -> Self {
        Self {
            position: defaults::orb_position(),
            size: defaults::orb_size(),
            opacity: defaults::orb_opacity(),
            always_on_top: defaults::orb_always_on_top(),
            click_through: defaults::orb_click_through(),
        }
    }
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            disconnected: defaults::color_disconnected(),
            idle: defaults::color_idle(),
            starting: defaults::color_starting(),
            active: defaults::color_active(),
            thinking_like: defaults::color_thinking_like(),
            waiting_input: defaults::color_waiting_input(),
            compacting: defaults::color_compacting(),
            completed: defaults::color_completed(),
            error: defaults::color_error(),
            warning: defaults::color_warning(),
        }
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            silent_threshold_seconds: defaults::silent_threshold_seconds(),
            stuck_threshold_seconds: defaults::stuck_threshold_seconds(),
            completed_hold_seconds: defaults::completed_hold_seconds(),
            error_requires_click_to_clear: defaults::error_requires_click_to_clear(),
        }
    }
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            include_output_sample: defaults::include_output_sample(),
            max_sample_chars: defaults::max_sample_chars(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read config: {err}"),
            Self::Parse(err) => write!(f, "failed to parse config: {err}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Parse(err) => Some(err),
        }
    }
}

fn strip_bom(input: &str) -> &str {
    input.strip_prefix('\u{feff}').unwrap_or(input)
}

mod defaults {
    pub fn daemon_host() -> String {
        "127.0.0.1".to_string()
    }

    pub const fn daemon_port() -> u16 {
        17321
    }

    pub const fn daemon_auto_start() -> bool {
        true
    }

    pub fn orb_position() -> String {
        "top-right".to_string()
    }

    pub const fn orb_size() -> u16 {
        48
    }

    pub const fn orb_opacity() -> f32 {
        0.88
    }

    pub const fn orb_always_on_top() -> bool {
        true
    }

    pub const fn orb_click_through() -> bool {
        false
    }

    pub fn color_disconnected() -> String {
        "#6B7280".to_string()
    }

    pub fn color_idle() -> String {
        "#9CA3AF".to_string()
    }

    pub fn color_starting() -> String {
        "#60A5FA".to_string()
    }

    pub fn color_active() -> String {
        "#3B82F6".to_string()
    }

    pub fn color_thinking_like() -> String {
        "#FACC15".to_string()
    }

    pub fn color_waiting_input() -> String {
        "#EF4444".to_string()
    }

    pub fn color_compacting() -> String {
        "#A855F7".to_string()
    }

    pub fn color_completed() -> String {
        "#22C55E".to_string()
    }

    pub fn color_error() -> String {
        "#EF4444".to_string()
    }

    pub fn color_warning() -> String {
        "#F97316".to_string()
    }

    pub const fn silent_threshold_seconds() -> u64 {
        20
    }

    pub const fn stuck_threshold_seconds() -> u64 {
        180
    }

    pub const fn completed_hold_seconds() -> u64 {
        10
    }

    pub const fn error_requires_click_to_clear() -> bool {
        true
    }

    pub const fn include_output_sample() -> bool {
        false
    }

    pub const fn max_sample_chars() -> usize {
        512
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_mvp_values() {
        let config = Config::default();

        assert_eq!(config.daemon.host, "127.0.0.1");
        assert_eq!(config.daemon.port, 17321);
        assert!(config.daemon.auto_start);
        assert_eq!(config.orb.position, "top-right");
        assert_eq!(config.orb.size, 48);
        assert_eq!(config.orb.opacity, 0.88);
        assert!(config.orb.always_on_top);
        assert!(!config.orb.click_through);
        assert_eq!(config.colors.active, "#3B82F6");
        assert_eq!(config.colors.thinking_like, "#FACC15");
        assert_eq!(config.colors.waiting_input, "#EF4444");
        assert_eq!(config.colors.compacting, "#A855F7");
        assert_eq!(config.behavior.silent_threshold_seconds, 20);
        assert_eq!(config.behavior.stuck_threshold_seconds, 180);
        assert_eq!(config.behavior.completed_hold_seconds, 10);
        assert!(config.behavior.error_requires_click_to_clear);
        assert!(!config.privacy.include_output_sample);
        assert_eq!(config.privacy.max_sample_chars, 512);
    }

    #[test]
    fn loads_example_config_with_unknown_sections_and_bom() {
        let config = Config::from_toml_str(include_str!("../../../examples/config.toml"))
            .expect("example config should parse");

        assert_eq!(config.daemon.host, "127.0.0.1");
        assert_eq!(config.daemon.port, 17321);
        assert_eq!(config.orb.position, "top-right");
        assert_eq!(config.behavior.silent_threshold_seconds, 20);
        assert!(!config.privacy.include_output_sample);
        assert_eq!(config.colors.error, "#EF4444");
    }

    #[test]
    fn missing_sections_fall_back_to_defaults() {
        let config = Config::from_toml_str(
            r##"
            [daemon]
            port = 18000

            [orb]
            size = 48

            [privacy]
            include_output_sample = true
            "##,
        )
        .expect("partial config should parse");

        assert_eq!(config.daemon.host, "127.0.0.1");
        assert_eq!(config.daemon.port, 18000);
        assert_eq!(config.orb.size, 48);
        assert_eq!(config.orb.position, "top-right");
        assert_eq!(config.colors.active, "#3B82F6");
        assert_eq!(config.behavior.completed_hold_seconds, 10);
        assert!(config.privacy.include_output_sample);
        assert_eq!(config.privacy.max_sample_chars, 512);
    }

    #[test]
    fn explicit_colors_override_defaults() {
        let config = Config::from_toml_str(
            r##"
            [colors]
            active = "#0000FF"
            error = "#FF0000"
            "##,
        )
        .expect("colors config should parse");

        assert_eq!(config.colors.active, "#0000FF");
        assert_eq!(config.colors.error, "#FF0000");
        assert_eq!(config.colors.idle, "#9CA3AF");
    }

    #[test]
    fn resolves_only_loopback_daemon_hosts() {
        assert_eq!(
            loopback_socket_addr("localhost", 17321),
            Some(SocketAddr::from(([127, 0, 0, 1], 17321)))
        );
        assert_eq!(
            loopback_socket_addr("127.0.0.2", 17321),
            Some(SocketAddr::from(([127, 0, 0, 2], 17321)))
        );
        assert!(loopback_socket_addr("::1", 17321).is_some());
        assert!(loopback_socket_addr("0.0.0.0", 17321).is_none());
        assert!(loopback_socket_addr("example.com", 17321).is_none());
    }
}
