use std::collections::HashMap;

use agent_orb_core::{
    event::{EventEnvelope, EventType},
    source::Source,
    state_machine::transition,
    status::InternalStatus,
    visual::VisualStatus,
};
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub source: Source,
    pub workspace: String,
    pub status: InternalStatus,
    pub started_at: String,
    pub updated_at: String,
    pub last_output_at: Option<String>,
    pub exit_code: Option<i64>,
    #[serde(skip)]
    updated_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub status: InternalStatus,
    pub visual: VisualStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct SessionStore {
    sessions: HashMap<String, Session>,
    sequence: u64,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_event(&mut self, event: EventEnvelope) -> StatusSnapshot {
        self.sequence += 1;
        let updated_seq = self.sequence;
        let current_status = self
            .sessions
            .get(&event.session_id)
            .map(|session| session.status)
            .unwrap_or(InternalStatus::Idle);
        let next_status = transition(current_status, &event);
        let exit_code = extract_exit_code(&event);

        let session = self
            .sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| Session {
                session_id: event.session_id.clone(),
                source: event.source.clone(),
                workspace: event.workspace.clone(),
                status: InternalStatus::Idle,
                started_at: event.timestamp.clone(),
                updated_at: event.timestamp.clone(),
                last_output_at: None,
                exit_code: None,
                updated_seq,
            });

        if event.event_type == EventType::SessionStarted {
            session.started_at = event.timestamp.clone();
            session.last_output_at = None;
            session.exit_code = None;
        }

        if matches!(
            event.event_type,
            EventType::OutputReceived | EventType::StderrReceived
        ) {
            session.last_output_at = Some(event.timestamp.clone());
        }

        if event.event_type == EventType::ProcessExited {
            session.exit_code = exit_code;
        }

        if event.event_type == EventType::SessionCleared {
            session.exit_code = None;
        }

        session.source = event.source;
        session.workspace = event.workspace;
        session.status = next_status;
        session.updated_at = event.timestamp;
        session.updated_seq = updated_seq;

        self.global_status()
    }

    pub fn global_status(&self) -> StatusSnapshot {
        let Some(session) = self
            .sessions
            .values()
            .max_by_key(|session| (status_priority(session.status), session.updated_seq))
        else {
            return StatusSnapshot::idle();
        };

        if session.status == InternalStatus::Idle {
            return StatusSnapshot::idle();
        }

        StatusSnapshot {
            status: session.status,
            visual: VisualStatus::from(session.status),
            source: Some(session.source.clone()),
            workspace: Some(session.workspace.clone()),
            session_id: Some(session.session_id.clone()),
            started_at: Some(session.started_at.clone()),
            updated_at: Some(session.updated_at.clone()),
            message: status_message(Some(&session.source), session.status),
        }
    }

    pub fn clear_terminal_statuses(&mut self) -> usize {
        let now = now_rfc3339();
        let mut cleared = 0;

        for session in self.sessions.values_mut() {
            if matches!(
                session.status,
                InternalStatus::Completed | InternalStatus::Failed
            ) {
                self.sequence += 1;
                session.status = InternalStatus::Idle;
                session.exit_code = None;
                session.updated_at = now.clone();
                session.updated_seq = self.sequence;
                cleared += 1;
            }
        }

        cleared
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    #[cfg(test)]
    pub fn get(&self, session_id: &str) -> Option<&Session> {
        self.sessions.get(session_id)
    }
}

impl StatusSnapshot {
    pub fn idle() -> Self {
        Self {
            status: InternalStatus::Idle,
            visual: VisualStatus::Idle,
            source: None,
            workspace: None,
            session_id: None,
            started_at: None,
            updated_at: None,
            message: "Agent Orb is idle".to_string(),
        }
    }
}

fn status_priority(status: InternalStatus) -> u8 {
    match status {
        InternalStatus::Failed => 80,
        InternalStatus::WaitingInput => 70,
        InternalStatus::Stuck => 60,
        InternalStatus::Active => 50,
        InternalStatus::Silent => 40,
        InternalStatus::Completed => 30,
        InternalStatus::Cancelled => 25,
        InternalStatus::Starting => 20,
        InternalStatus::Idle => 10,
        InternalStatus::Disconnected => 0,
    }
}

fn extract_exit_code(event: &EventEnvelope) -> Option<i64> {
    event
        .payload
        .get("exit_code")
        .and_then(serde_json::Value::as_i64)
}

fn status_message(source: Option<&Source>, status: InternalStatus) -> String {
    let subject = source.map(source_label).unwrap_or("Agent Orb");
    let phrase = match status {
        InternalStatus::Disconnected => "is disconnected",
        InternalStatus::Idle => "is idle",
        InternalStatus::Starting => "is starting",
        InternalStatus::Active => "is active",
        InternalStatus::Silent => "is silent",
        InternalStatus::WaitingInput => "may be waiting for input",
        InternalStatus::Completed => "completed successfully",
        InternalStatus::Failed => "failed",
        InternalStatus::Stuck => "may be stuck",
        InternalStatus::Cancelled => "was cancelled",
    };

    format!("{subject} {phrase}")
}

fn source_label(source: &Source) -> &'static str {
    match source {
        Source::Codex => "Codex",
        Source::Claude => "Claude",
        Source::Generic => "Agent",
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_orb_core::{event::EventType, source::Source};
    use serde_json::json;

    fn event(
        session_id: &str,
        source: Source,
        event_type: EventType,
        timestamp: &str,
    ) -> EventEnvelope {
        EventEnvelope {
            version: "1.0".to_string(),
            event_id: format!("event-{session_id}-{timestamp}"),
            session_id: session_id.to_string(),
            source,
            workspace: format!("/tmp/{session_id}"),
            event_type,
            timestamp: timestamp.to_string(),
            payload: json!({}),
        }
    }

    fn exit_event(
        session_id: &str,
        source: Source,
        exit_code: i64,
        timestamp: &str,
    ) -> EventEnvelope {
        EventEnvelope {
            payload: json!({ "exit_code": exit_code }),
            ..event(session_id, source, EventType::ProcessExited, timestamp)
        }
    }

    #[test]
    fn applies_events_and_tracks_session_fields() {
        let mut store = SessionStore::new();

        store.apply_event(event(
            "s1",
            Source::Codex,
            EventType::SessionStarted,
            "2026-06-29T12:00:00+08:00",
        ));
        let snapshot = store.apply_event(event(
            "s1",
            Source::Codex,
            EventType::OutputReceived,
            "2026-06-29T12:00:01+08:00",
        ));

        assert_eq!(snapshot.status, InternalStatus::Active);
        assert_eq!(store.len(), 1);
        let session = store.get("s1").expect("session should exist");
        assert_eq!(session.started_at, "2026-06-29T12:00:00+08:00");
        assert_eq!(
            session.last_output_at.as_deref(),
            Some("2026-06-29T12:00:01+08:00")
        );
    }

    #[test]
    fn failed_has_priority_over_active() {
        let mut store = SessionStore::new();

        store.apply_event(event(
            "active",
            Source::Codex,
            EventType::SessionStarted,
            "2026-06-29T12:00:00+08:00",
        ));
        store.apply_event(event(
            "active",
            Source::Codex,
            EventType::OutputReceived,
            "2026-06-29T12:00:01+08:00",
        ));
        store.apply_event(event(
            "failed",
            Source::Claude,
            EventType::SessionStarted,
            "2026-06-29T12:00:02+08:00",
        ));
        let snapshot = store.apply_event(exit_event(
            "failed",
            Source::Claude,
            1,
            "2026-06-29T12:00:03+08:00",
        ));

        assert_eq!(snapshot.status, InternalStatus::Failed);
        assert_eq!(snapshot.source, Some(Source::Claude));
    }

    #[test]
    fn same_priority_uses_most_recent_session() {
        let mut store = SessionStore::new();

        store.apply_event(event(
            "old",
            Source::Codex,
            EventType::SessionStarted,
            "2026-06-29T12:00:00+08:00",
        ));
        store.apply_event(event(
            "old",
            Source::Codex,
            EventType::OutputReceived,
            "2026-06-29T12:00:01+08:00",
        ));
        store.apply_event(event(
            "new",
            Source::Claude,
            EventType::SessionStarted,
            "2026-06-29T12:00:02+08:00",
        ));
        let snapshot = store.apply_event(event(
            "new",
            Source::Claude,
            EventType::OutputReceived,
            "2026-06-29T12:00:03+08:00",
        ));

        assert_eq!(snapshot.status, InternalStatus::Active);
        assert_eq!(snapshot.session_id.as_deref(), Some("new"));
    }

    #[test]
    fn clear_terminal_statuses_returns_to_idle_when_only_failed_exists() {
        let mut store = SessionStore::new();

        store.apply_event(event(
            "s1",
            Source::Codex,
            EventType::SessionStarted,
            "2026-06-29T12:00:00+08:00",
        ));
        store.apply_event(exit_event(
            "s1",
            Source::Codex,
            1,
            "2026-06-29T12:00:01+08:00",
        ));

        assert_eq!(store.global_status().status, InternalStatus::Failed);
        assert_eq!(store.clear_terminal_statuses(), 1);
        assert_eq!(store.global_status().status, InternalStatus::Idle);
    }
}
