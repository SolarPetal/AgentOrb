use serde::{Deserialize, Serialize};

use crate::source::Source;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    #[serde(rename = "session.started")]
    SessionStarted,
    #[serde(rename = "output.received")]
    OutputReceived,
    #[serde(rename = "stderr.received")]
    StderrReceived,
    #[serde(rename = "prompt.detected")]
    PromptDetected,
    #[serde(rename = "idle.timeout")]
    IdleTimeout,
    #[serde(rename = "stuck.timeout")]
    StuckTimeout,
    #[serde(rename = "process.exited")]
    ProcessExited,
    #[serde(rename = "session.cancelled")]
    SessionCancelled,
    #[serde(rename = "session.cleared")]
    SessionCleared,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub version: String,
    pub event_id: String,
    pub session_id: String,
    pub source: Source,
    pub workspace: String,
    pub event_type: EventType,
    pub timestamp: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

impl EventEnvelope {
    pub fn from_json_str(input: &str) -> serde_json::Result<Self> {
        serde_json::from_str(strip_bom(input))
    }
}

fn strip_bom(input: &str) -> &str {
    input.strip_prefix('\u{feff}').unwrap_or(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_events() {
        let cases = [
            (
                include_str!("../../../examples/events/session-started.json"),
                EventType::SessionStarted,
            ),
            (
                include_str!("../../../examples/events/output-received.json"),
                EventType::OutputReceived,
            ),
            (
                include_str!("../../../examples/events/process-exited.json"),
                EventType::ProcessExited,
            ),
        ];

        for (input, expected_event_type) in cases {
            let event = EventEnvelope::from_json_str(input).expect("example event should parse");

            assert_eq!(event.version, "1.0");
            assert_eq!(event.source, Source::Codex);
            assert_eq!(event.event_type, expected_event_type);
            assert_eq!(event.workspace, "E:/code/project");
        }
    }

    #[test]
    fn unknown_event_type_returns_error() {
        let result = EventEnvelope::from_json_str(
            r#"
            {
              "version": "1.0",
              "event_id": "event-id",
              "session_id": "session-id",
              "source": "codex",
              "workspace": "/tmp/project",
              "event_type": "unknown.event",
              "timestamp": "2026-06-29T12:00:00+08:00",
              "payload": {}
            }
            "#,
        );

        assert!(result.is_err());
    }
}
