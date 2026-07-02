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
        let groups = [
            (
                AdapterStatusHint::Compacting,
                4_u8,
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
                ][..],
            ),
            (
                AdapterStatusHint::Waiting,
                3_u8,
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
                ][..],
            ),
            (
                AdapterStatusHint::Thinking,
                2_u8,
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
                ][..],
            ),
            (
                AdapterStatusHint::Executing,
                1_u8,
                &[
                    "running tool",
                    "executing tool",
                    "using tool",
                    "tool use",
                    "tool call",
                    "running command",
                    "executing command",
                    "shell command",
                    "bash(",
                    "powershell(",
                    "cmd.exe",
                    "apply_patch",
                    "applying patch",
                    "editing file",
                    "writing file",
                    "reading file",
                    "searching files",
                    "fetching url",
                    "call tool",
                    "exec command",
                    "运行工具",
                    "执行工具",
                    "执行命令",
                    "读取文件",
                    "写入文件",
                ][..],
            ),
        ];

        groups
            .iter()
            .filter_map(|(hint, priority, patterns)| {
                latest_match_index(lower, patterns).map(|index| (index, *priority, *hint))
            })
            .max_by_key(|(index, priority, _)| (*index, *priority))
            .map(|(_, _, hint)| hint)
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

fn latest_match_index(text: &str, patterns: &[&str]) -> Option<usize> {
    patterns
        .iter()
        .filter_map(|pattern| text.rfind(pattern))
        .max()
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

    #[test]
    fn status_detector_prefers_current_thinking_over_old_tool_text() {
        let detector = StatusDetector::for_source(&Source::Claude);

        assert_eq!(
            detector
                .detect("Previous tool: Bash(command)\n\x1b[33mThinking… esc to interrupt\x1b[0m"),
            Some(AdapterStatusHint::Thinking)
        );
    }

    #[test]
    fn status_detector_prefers_latest_status_hint() {
        let detector = StatusDetector::for_source(&Source::Claude);

        assert_eq!(
            detector.detect("Thinking…\nRunning tool: bash"),
            Some(AdapterStatusHint::Executing)
        );
    }
}
