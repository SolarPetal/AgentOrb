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

        // Interactive CLIs repaint the live status line at the bottom of the
        // terminal, so the most recent state is on the last non-empty line.
        // Scan lines from the bottom up and return the first line that carries
        // any status signal; within a line, group order (highest priority
        // first) breaks ties. This prevents stale tool output higher up in a
        // full-screen repaint from pinning the orb to an old state.
        normalized
            .lines()
            .rev()
            .filter(|line| !line.trim().is_empty())
            .find_map(detect_status_in_line)
    }
}

/// Status groups ordered by descending priority. The first group that matches a
/// line wins, so ordering encodes precedence: compacting > waiting > thinking >
/// executing.
fn status_groups() -> [(AdapterStatusHint, &'static [&'static str]); 4] {
    [
        (
            AdapterStatusHint::Compacting,
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
                // Claude Code renders a randomized gerund next to its spinner;
                // the stable "esc to interrupt" hint above is the primary anchor,
                // these cover the common spinner verbs when it is off-screen.
                "ruminating",
                "puzzling",
                "cogitating",
                "percolating",
                "noodling",
                "pondering",
                "mulling",
                "brewing",
                "churning",
                "synthesizing",
                "deliberating",
                "思考",
                "分析",
                "计划",
                "推理",
            ][..],
        ),
        (
            AdapterStatusHint::Executing,
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
    ]
}

fn detect_status_in_line(line: &str) -> Option<AdapterStatusHint> {
    status_groups()
        .into_iter()
        .find(|(_, patterns)| patterns.iter().any(|pattern| line.contains(pattern)))
        .map(|(hint, _)| hint)
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
        let normalized = normalize_terminal_text(text);
        // A prompt is a live question at the bottom of the screen. Scan from the
        // last non-empty line up so a stale prompt higher in a repaint cannot
        // outrank fresh output that already scrolled past it.
        normalized
            .lines()
            .rev()
            .filter(|line| !line.trim().is_empty())
            .find_map(|line| self.patterns.iter().copied().find(|p| line.contains(p)))
    }
}

pub fn truncate_output_sample(bytes: &[u8], max_sample_chars: usize) -> String {
    let sample = String::from_utf8_lossy(bytes);
    truncate_chars(sample.as_ref(), max_sample_chars)
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

    #[test]
    fn full_screen_repaint_uses_bottom_live_status_line() {
        let detector = StatusDetector::for_source(&Source::Claude);

        // Realistic Claude repaint: old completed tool output scrolled up top,
        // the live thinking spinner painted on the bottom line. The old
        // "latest keyword wins" logic wrongly reported Executing here.
        let frame = concat!(
            "\x1b[2J\x1b[H",
            "> summarize the repo\n",
            "● Bash(ls -la)\n",
            "  ⎿  Reading file src/main.rs\n",
            "\n",
            "\x1b[33m✻ Ruminating… (12s · esc to interrupt)\x1b[0m",
        );
        assert_eq!(detector.detect(frame), Some(AdapterStatusHint::Thinking));
    }

    #[test]
    fn random_claude_spinner_verb_is_thinking() {
        let detector = StatusDetector::for_source(&Source::Claude);

        for verb in ["Puzzling", "Percolating", "Noodling", "Cogitating"] {
            let line = format!("\x1b[33m✻ {verb}… (esc to interrupt)\x1b[0m");
            assert_eq!(
                detector.detect(&line),
                Some(AdapterStatusHint::Thinking),
                "spinner verb {verb} should map to Thinking"
            );
        }
    }

    #[test]
    fn waiting_prompt_at_bottom_beats_earlier_tool_output() {
        let detector = StatusDetector::for_source(&Source::Claude);

        let frame = concat!(
            "● Bash(rm -rf build)\n",
            "  Do you want to proceed? (y/n)",
        );
        assert_eq!(detector.detect(frame), Some(AdapterStatusHint::Waiting));
    }
}
