use agent_orb_core::source::Source;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterStatusHint {
    Thinking,
    Executing,
    Waiting,
    Compacting,
}

impl AdapterStatusHint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Thinking => "thinking",
            Self::Executing => "executing",
            Self::Waiting => "waiting",
            Self::Compacting => "compacting",
        }
    }
}

#[derive(Debug, Clone)]
pub struct StatusDetector;

impl StatusDetector {
    pub fn for_source(_source: &Source) -> Self {
        Self
    }

    pub fn detect(&self, text: &str) -> Option<AdapterStatusHint> {
        let normalized = normalize_terminal_text(text);
        let lower = normalized.as_str();

        if contains_any(
            lower,
            &[
                "compacting",
                "compact",
                "auto-compact",
                "auto compact",
                "condensing",
                "compressing context",
                "context compression",
                "summarizing conversation",
                "summarising conversation",
                "压缩",
                "压缩上下文",
                "总结上下文",
            ],
        ) {
            return Some(AdapterStatusHint::Compacting);
        }

        if contains_any(
            lower,
            &[
                "approve",
                "approval",
                "allow",
                "deny",
                "permission",
                "permissions",
                "confirm",
                "continue?",
                "yes/no",
                "y/n",
                "press enter",
                "press return",
                "do you want",
                "proceed?",
                "waiting for input",
                "需要确认",
                "等待输入",
                "是否继续",
            ],
        ) {
            return Some(AdapterStatusHint::Waiting);
        }

        if contains_any(
            lower,
            &[
                "running",
                "executing",
                "execute",
                "shell",
                "bash",
                "powershell",
                "cmd.exe",
                "command",
                "tool",
                "apply_patch",
                "patch",
                "editing",
                "writing",
                "reading",
                "searching",
                "fetching",
                "call",
                "exec",
                "运行",
                "执行",
                "读取",
                "写入",
                "工具",
            ],
        ) {
            return Some(AdapterStatusHint::Executing);
        }

        if contains_any(
            lower,
            &[
                "thinking",
                "reasoning",
                "analyzing",
                "analysing",
                "planning",
                "processing",
                "pondering",
                "working",
                "esc to interrupt",
                "思考",
                "分析",
                "计划",
                "推理",
            ],
        ) {
            return Some(AdapterStatusHint::Thinking);
        }

        None
    }
}

#[derive(Debug, Clone)]
pub struct PromptDetector {
    patterns: Vec<&'static str>,
}

impl PromptDetector {
    pub fn for_source(source: &Source) -> Self {
        let mut patterns = vec![
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
        let lower = normalize_terminal_text(text);
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

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| text.contains(pattern))
}

fn normalize_terminal_text(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            skip_ansi_sequence(&mut chars);
            continue;
        }

        if matches!(ch, '\r' | '\u{0008}' | '\u{0007}') {
            continue;
        }

        if ch.is_control() && !matches!(ch, '\n' | '\t') {
            continue;
        }

        output.extend(ch.to_lowercase());
    }

    output
}

fn skip_ansi_sequence<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    if matches!(chars.peek(), Some('[' | ']' | '(' | ')' | 'P')) {
        chars.next();
    }

    for ch in chars.by_ref() {
        if ch == '\u{0007}' || ch.is_ascii_alphabetic() || matches!(ch, '~') {
            break;
        }
    }
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

        assert_eq!(
            detector.detect("Approve this command? [yes/no]"),
            Some("yes/no")
        );
        assert_eq!(detector.detect("regular output"), None);
    }

    #[test]
    fn output_sample_is_bounded_and_prompt_detectable() {
        let detector = PromptDetector::for_source(&Source::Generic);

        assert_eq!(
            detector.detect(&truncate_output_sample(b"continue? yes/no", 512)),
            Some("continue?")
        );
        assert_eq!(truncate_output_sample(b"abcdef", 3), "abc");
    }

    #[test]
    fn status_detector_finds_six_state_hints() {
        let detector = StatusDetector::for_source(&Source::Claude);

        assert_eq!(
            detector.detect("\x1b[33mThinking…\x1b[0m"),
            Some(AdapterStatusHint::Thinking)
        );
        assert_eq!(
            detector.detect("Running tool: bash"),
            Some(AdapterStatusHint::Executing)
        );
        assert_eq!(
            detector.detect("Do you want to proceed? yes/no"),
            Some(AdapterStatusHint::Waiting)
        );
        assert_eq!(
            detector.detect("Compacting context"),
            Some(AdapterStatusHint::Compacting)
        );
        assert_eq!(detector.detect("plain answer text"), None);
    }
}
