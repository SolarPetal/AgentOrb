use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InternalStatus {
    Disconnected,
    Idle,
    Starting,
    Active,
    Silent,
    WaitingInput,
    Completed,
    Failed,
    Stuck,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_status_as_api_snake_case() {
        assert_eq!(
            serde_json::to_string(&InternalStatus::Active).expect("status should serialize"),
            "\"active\""
        );
        assert_eq!(
            serde_json::to_string(&InternalStatus::WaitingInput).expect("status should serialize"),
            "\"waiting_input\""
        );
    }
}
