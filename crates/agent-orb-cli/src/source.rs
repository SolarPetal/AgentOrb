use std::path::Path;

use agent_orb_core::source::Source;

pub fn detect_source(command: &str) -> Source {
    let file_name = Path::new(command)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .to_ascii_lowercase();

    if file_name.contains("codex") {
        Source::Codex
    } else if file_name.contains("claude") {
        Source::Claude
    } else {
        Source::Generic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_known_sources_from_command_name() {
        assert_eq!(detect_source("codex"), Source::Codex);
        assert_eq!(detect_source("/usr/local/bin/claude"), Source::Claude);
        assert_eq!(detect_source("echo"), Source::Generic);
    }
}
