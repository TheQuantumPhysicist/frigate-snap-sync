pub mod recordings_state;
pub mod reviews;
pub mod snapshot;
pub mod snapshots_state;

mod utils;

use crate::config::MqttHandlerConfig;
use recordings_state::RecordingsState;
use reviews::Reviews;
use snapshot::Snapshot;
use snapshots_state::SnapshotsState;

#[must_use]
#[derive(Debug, Clone)]
pub enum CapturedPayloads {
    CameraRecordingsState(RecordingsState),
    CameraSnapshotsState(SnapshotsState),
    Snapshot(Snapshot),
    Reviews(Box<Reviews>),
}

impl CapturedPayloads {
    pub fn from_publish(
        config: &MqttHandlerConfig,
        topic: &str,
        payload: &bytes::Bytes,
    ) -> Option<Self> {
        let topic_parts = topic.split('/').collect::<Vec<_>>();
        if !topic_parts.is_empty() && topic_parts[0] == config.mqtt_frigate_topic_prefix {
            // Do nothing
        } else {
            return None;
        }

        if let Some(o) = SnapshotsState::from_topic_parts(&topic_parts, payload) {
            tracing::debug!("Parsed success: SnapshotsState");
            return Some(Self::CameraSnapshotsState(o));
        }

        if let Some(o) = RecordingsState::from_topic_parts(&topic_parts, payload) {
            tracing::debug!("Parsed success: RecordingsState");
            return Some(Self::CameraRecordingsState(o));
        }

        if let Some(o) = Snapshot::from_topic_parts(&topic_parts, payload) {
            tracing::debug!("Parsed success: Snapshot");
            return Some(Self::Snapshot(o));
        }

        if let Some(o) = Reviews::from_topic_parts(&topic_parts, payload) {
            tracing::debug!("Parsed success: Reviews");
            return Some(Self::Reviews(o.into()));
        }

        tracing::debug!("Ignoring message with topic: {topic}");

        None
    }

    #[must_use]
    pub fn into_recordings_state(self) -> Option<RecordingsState> {
        match self {
            CapturedPayloads::CameraRecordingsState(r) => Some(r),
            _ => None,
        }
    }

    #[must_use]
    pub fn into_snapshots_state(self) -> Option<SnapshotsState> {
        match self {
            CapturedPayloads::CameraSnapshotsState(r) => Some(r),
            _ => None,
        }
    }

    #[must_use]
    pub fn into_snapshot(self) -> Option<Snapshot> {
        match self {
            CapturedPayloads::Snapshot(s) => Some(s),
            _ => None,
        }
    }
}
