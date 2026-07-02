use serde::{Deserialize, Serialize};

use crate::status::InternalStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualStatus {
    Disconnected,
    Idle,
    Starting,
    BlueSpinning,
    YellowThinking,
    RedWaiting,
    PurpleCompacting,
    PurpleSpinning,
    YellowPulse,
    GreenDone,
    RedError,
    OrangeWarning,
    Cancelled,
}

impl From<InternalStatus> for VisualStatus {
    fn from(status: InternalStatus) -> Self {
        match status {
            InternalStatus::Disconnected => Self::Disconnected,
            InternalStatus::Idle => Self::Idle,
            InternalStatus::Starting => Self::Starting,
            InternalStatus::Active => Self::BlueSpinning,
            InternalStatus::Silent => Self::YellowThinking,
            InternalStatus::WaitingInput => Self::RedWaiting,
            InternalStatus::Completed => Self::GreenDone,
            InternalStatus::Compacting => Self::PurpleCompacting,
            InternalStatus::Failed => Self::RedError,
            InternalStatus::Stuck => Self::OrangeWarning,
            InternalStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_internal_status_to_visual_status() {
        let cases = [
            (InternalStatus::Disconnected, VisualStatus::Disconnected),
            (InternalStatus::Idle, VisualStatus::Idle),
            (InternalStatus::Starting, VisualStatus::Starting),
            (InternalStatus::Active, VisualStatus::BlueSpinning),
            (InternalStatus::Silent, VisualStatus::YellowThinking),
            (InternalStatus::WaitingInput, VisualStatus::RedWaiting),
            (InternalStatus::Completed, VisualStatus::GreenDone),
            (InternalStatus::Compacting, VisualStatus::PurpleCompacting),
            (InternalStatus::Failed, VisualStatus::RedError),
            (InternalStatus::Stuck, VisualStatus::OrangeWarning),
            (InternalStatus::Cancelled, VisualStatus::Cancelled),
        ];

        for (internal, visual) in cases {
            assert_eq!(VisualStatus::from(internal), visual);
        }
    }
}
