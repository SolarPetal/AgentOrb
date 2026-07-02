use crate::{
    event::{EventEnvelope, EventType},
    source::Source,
    status::InternalStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateMachine {
    status: InternalStatus,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            status: InternalStatus::Idle,
        }
    }

    pub fn with_status(status: InternalStatus) -> Self {
        Self { status }
    }

    pub fn status(&self) -> InternalStatus {
        self.status
    }

    pub fn apply(&mut self, event: &EventEnvelope) -> InternalStatus {
        self.status = transition(self.status, event);
        self.status
    }
}

pub fn transition(current: InternalStatus, event: &EventEnvelope) -> InternalStatus {
    use EventType::*;
    use InternalStatus::*;

    match event.event_type {
        SessionStarted => {
            if is_observed_adapter_session(event) {
                Idle
            } else {
                Starting
            }
        }
        SessionCleared => Idle,
        SessionCancelled => Cancelled,
        OutputReceived | StderrReceived if is_adapter_terminal_paint(event) => current,
        OutputReceived | StderrReceived => match current {
            Starting | Active | Silent | WaitingInput | Compacting => Active,
            other => other,
        },
        StatusHint => status_hint(event).unwrap_or(current),
        IdleTimeout => match current {
            Active | Starting => Silent,
            other => other,
        },
        PromptDetected => match current {
            Idle | Starting | Active | Silent | WaitingInput => WaitingInput,
            other => other,
        },
        StuckTimeout => match current {
            Silent => Stuck,
            other => other,
        },
        ProcessExited => match current {
            Idle if !process_exit_succeeded(event) => Failed,
            Starting | Active | Silent | WaitingInput | Compacting | Stuck => {
                if process_exit_succeeded(event) {
                    Completed
                } else {
                    Failed
                }
            }
            other => other,
        },
    }
}

fn is_interactive_adapter(source: &Source) -> bool {
    matches!(source, Source::Codex | Source::Claude)
}

fn is_observed_adapter_session(event: &EventEnvelope) -> bool {
    is_interactive_adapter(&event.source)
        && matches!(
            event
                .payload
                .get("stdio")
                .and_then(serde_json::Value::as_str),
            Some("pty")
        )
}

fn is_adapter_terminal_paint(event: &EventEnvelope) -> bool {
    if !is_interactive_adapter(&event.source) {
        return false;
    }

    let stream = event
        .payload
        .get("stream")
        .and_then(serde_json::Value::as_str);
    let synthetic = event
        .payload
        .get("synthetic")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    matches!(stream, Some("pty")) || synthetic
}

fn status_hint(event: &EventEnvelope) -> Option<InternalStatus> {
    match event.payload.get("status")?.as_str()? {
        "idle" => Some(InternalStatus::Idle),
        "thinking" => Some(InternalStatus::Silent),
        "executing" => Some(InternalStatus::Active),
        "waiting" => Some(InternalStatus::WaitingInput),
        "completed" => Some(InternalStatus::Completed),
        "compacting" => Some(InternalStatus::Compacting),
        _ => None,
    }
}

fn process_exit_succeeded(event: &EventEnvelope) -> bool {
    event
        .payload
        .get("exit_code")
        .and_then(serde_json::Value::as_i64)
        .is_some_and(|code| code == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use serde_json::json;

    fn event(event_type: EventType) -> EventEnvelope {
        EventEnvelope {
            version: "1.0".to_string(),
            event_id: "event-id".to_string(),
            session_id: "session-id".to_string(),
            source: Source::Generic,
            workspace: "/tmp/project".to_string(),
            event_type,
            timestamp: "2026-06-29T12:00:00+08:00".to_string(),
            payload: json!({}),
        }
    }

    fn exit_event(exit_code: i32) -> EventEnvelope {
        EventEnvelope {
            payload: json!({ "exit_code": exit_code }),
            ..event(EventType::ProcessExited)
        }
    }

    #[test]
    fn happy_path_reaches_completed() {
        let mut machine = StateMachine::new();

        assert_eq!(
            machine.apply(&event(EventType::SessionStarted)),
            InternalStatus::Starting
        );
        assert_eq!(
            machine.apply(&event(EventType::OutputReceived)),
            InternalStatus::Active
        );
        assert_eq!(machine.apply(&exit_event(0)), InternalStatus::Completed);
    }

    #[test]
    fn non_zero_exit_reaches_failed() {
        let mut machine = StateMachine::with_status(InternalStatus::Active);

        assert_eq!(machine.apply(&exit_event(2)), InternalStatus::Failed);
    }

    #[test]
    fn idle_timeout_moves_active_to_silent() {
        let mut machine = StateMachine::with_status(InternalStatus::Active);

        assert_eq!(
            machine.apply(&event(EventType::IdleTimeout)),
            InternalStatus::Silent
        );
    }

    #[test]
    fn prompt_detected_moves_silent_to_waiting_input() {
        let mut machine = StateMachine::with_status(InternalStatus::Silent);

        assert_eq!(
            machine.apply(&event(EventType::PromptDetected)),
            InternalStatus::WaitingInput
        );
    }

    #[test]
    fn output_moves_waiting_input_back_to_active() {
        let mut machine = StateMachine::with_status(InternalStatus::WaitingInput);

        assert_eq!(
            machine.apply(&event(EventType::OutputReceived)),
            InternalStatus::Active
        );
    }

    #[test]
    fn status_hint_maps_adapter_states() {
        let mut machine = StateMachine::with_status(InternalStatus::Active);

        let hint = EventEnvelope {
            payload: json!({ "status": "compacting" }),
            ..event(EventType::StatusHint)
        };
        assert_eq!(machine.apply(&hint), InternalStatus::Compacting);

        let hint = EventEnvelope {
            payload: json!({ "status": "waiting" }),
            ..event(EventType::StatusHint)
        };
        assert_eq!(machine.apply(&hint), InternalStatus::WaitingInput);

        let hint = EventEnvelope {
            payload: json!({ "status": "thinking" }),
            ..event(EventType::StatusHint)
        };
        assert_eq!(machine.apply(&hint), InternalStatus::Silent);
    }

    #[test]
    fn adapter_start_is_idle_until_cli_state_hint_arrives() {
        let mut machine = StateMachine::new();

        let started = EventEnvelope {
            source: Source::Claude,
            payload: json!({ "stdio": "pty" }),
            ..event(EventType::SessionStarted)
        };
        assert_eq!(machine.apply(&started), InternalStatus::Idle);

        let terminal_paint = EventEnvelope {
            source: Source::Claude,
            payload: json!({ "stream": "pty", "bytes": 128 }),
            ..event(EventType::OutputReceived)
        };
        assert_eq!(machine.apply(&terminal_paint), InternalStatus::Idle);

        let thinking = EventEnvelope {
            source: Source::Claude,
            payload: json!({ "status": "thinking" }),
            ..event(EventType::StatusHint)
        };
        assert_eq!(machine.apply(&thinking), InternalStatus::Silent);
        assert_eq!(machine.apply(&terminal_paint), InternalStatus::Silent);

        let executing = EventEnvelope {
            source: Source::Claude,
            payload: json!({ "status": "executing" }),
            ..event(EventType::StatusHint)
        };
        assert_eq!(machine.apply(&executing), InternalStatus::Active);
    }

    #[test]
    fn adapter_crash_from_idle_is_failed() {
        let mut machine = StateMachine::new();

        let started = EventEnvelope {
            source: Source::Claude,
            payload: json!({ "stdio": "pty" }),
            ..event(EventType::SessionStarted)
        };
        assert_eq!(machine.apply(&started), InternalStatus::Idle);

        let failed_exit = EventEnvelope {
            source: Source::Claude,
            payload: json!({ "exit_code": 2 }),
            ..event(EventType::ProcessExited)
        };
        assert_eq!(machine.apply(&failed_exit), InternalStatus::Failed);
    }

    #[test]
    fn stderr_counts_as_activity() {
        let mut machine = StateMachine::with_status(InternalStatus::Silent);

        assert_eq!(
            machine.apply(&event(EventType::StderrReceived)),
            InternalStatus::Active
        );
    }

    #[test]
    fn stuck_timeout_only_moves_silent_to_stuck() {
        let mut silent_machine = StateMachine::with_status(InternalStatus::Silent);
        let mut active_machine = StateMachine::with_status(InternalStatus::Active);

        assert_eq!(
            silent_machine.apply(&event(EventType::StuckTimeout)),
            InternalStatus::Stuck
        );
        assert_eq!(
            active_machine.apply(&event(EventType::StuckTimeout)),
            InternalStatus::Active
        );
    }

    #[test]
    fn cancelled_and_cleared_are_explicit_events() {
        let mut machine = StateMachine::with_status(InternalStatus::Active);

        assert_eq!(
            machine.apply(&event(EventType::SessionCancelled)),
            InternalStatus::Cancelled
        );
        assert_eq!(
            machine.apply(&event(EventType::SessionCleared)),
            InternalStatus::Idle
        );
    }

    #[test]
    fn missing_exit_code_is_failed() {
        let mut machine = StateMachine::with_status(InternalStatus::Active);

        assert_eq!(
            machine.apply(&event(EventType::ProcessExited)),
            InternalStatus::Failed
        );
    }
}
