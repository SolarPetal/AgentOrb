use agent_orb_core::{
    event::{EventEnvelope, EventType},
    source::Source,
};
use serde_json::Value;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

pub fn build_event(
    session_id: &str,
    source: Source,
    workspace: String,
    event_type: EventType,
    payload: Value,
) -> EventEnvelope {
    EventEnvelope {
        version: "1.0".to_string(),
        event_id: Uuid::now_v7().to_string(),
        session_id: session_id.to_string(),
        source,
        workspace,
        event_type,
        timestamp: now_rfc3339(),
        payload,
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

pub fn json_without_nulls(mut value: Value) -> Value {
    if let Value::Object(ref mut object) = value {
        object.retain(|_, value| !value.is_null());
    }
    value
}
