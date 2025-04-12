mod recordings_state;
mod snapshot;
mod snapshots_state;
mod utils;

use crate::config::VideoSyncConfig;
use recordings_state::RecordingsState;
use snapshot::Snapshot;
use snapshots_state::SnapshotsState;

#[must_use]
#[derive(Debug)]
pub enum CapturedPayloads {
    CameraRecordingsState(RecordingsState),
    CameraSnapshotsState(SnapshotsState),
    Snapshot(Snapshot),
}

impl CapturedPayloads {
    pub fn from_publish(
        config: &VideoSyncConfig,
        topic: &str,
        payload: &bytes::Bytes,
    ) -> Option<Self> {
        let topic_parts = topic.split('/').collect::<Vec<_>>();
        if !topic_parts.is_empty() && topic_parts[0] == config.mqtt_frigate_topic_prefix() {
            // Do nothing
        } else {
            return None;
        }

        if let Some(o) = SnapshotsState::from_topic_parts(&topic_parts, payload) {
            return Some(Self::CameraSnapshotsState(o));
        }

        if let Some(o) = RecordingsState::from_topic_parts(&topic_parts, payload) {
            return Some(Self::CameraRecordingsState(o));
        }

        if let Some(o) = Snapshot::from_topic_parts(&topic_parts, payload) {
            return Some(Self::Snapshot(o));
        }

        tracing::debug!("Ignoring message with topic: {topic}");

        None
    }

    fn into_recordings_state(self) -> Option<RecordingsState> {
        match self {
            CapturedPayloads::CameraRecordingsState(r) => Some(r),
            _ => None,
        }
    }

    fn into_snapshots_state(self) -> Option<SnapshotsState> {
        match self {
            CapturedPayloads::CameraSnapshotsState(r) => Some(r),
            _ => None,
        }
    }
}
