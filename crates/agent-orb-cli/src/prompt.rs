use agent_orb_core::source::Source;

#[derive(Debug, Clone)]
pub struct PromptDetector {
    patterns: Vec<&'static str>,
}

impl PromptDetector {
    pub fn for_source(source: &Source) -> Self {
        let mut patterns = vec![
            "?",
            "confirm",
            "continue?",
            "yes/no",
            "approve",
            "permission",
            "press enter",
        ];

        match source {
            Source::Codex => patterns.extend(["approval", "allow", "deny"]),
            Source::Claude => {
                patterns.extend(["do you want to proceed", "proceed?", "press enter"])
            }
            Source::Generic => {}
        }

        Self { patterns }
    }

    pub fn detect(&self, text: &str) -> Option<&'static str> {
        let lower = text.to_ascii_lowercase();
        self.patterns
            .iter()
            .copied()
            .find(|pattern| lower.contains(pattern))
    }
}

pub fn truncate_output_sample(bytes: &[u8], max_sample_chars: usize) -> String {
    let sample = String::from_utf8_lossy(bytes);
    truncate_chars(sample.as_ref(), max_sample_chars)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_detector_finds_common_prompts() {
        let detector = PromptDetector::for_source(&Source::Codex);

        assert_eq!(detector.detect("Approve this command? [yes/no]"), Some("?"));
        assert_eq!(detector.detect("regular output"), None);
    }

    #[test]
    fn output_sample_is_bounded_and_prompt_detectable() {
        let detector = PromptDetector::for_source(&Source::Generic);

        assert_eq!(
            detector.detect(&truncate_output_sample(b"continue? yes/no", 512)),
            Some("?")
        );
        assert_eq!(truncate_output_sample(b"abcdef", 3), "abc");
    }
}
