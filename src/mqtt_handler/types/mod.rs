use tap::TapOptional;

use crate::config::VideoSyncConfig;

#[must_use]
#[derive(Debug, PartialEq, Eq)]
struct SnapshotsState {
    camera_label: String,
    state: bool,
}

impl SnapshotsState {
    #[must_use]
    pub fn from_topic_parts(topic_parts: &[&str], payload: &bytes::Bytes) -> Option<Self> {
        if topic_parts.len() > 3 && topic_parts[2] == "snapshots" && topic_parts[3] == "state" {
            let camera_label = topic_parts[1].to_string();
            let state = on_off_from_bytes(payload.to_vec()).tap_none(|| {
                tracing::error!("Failed to parse snapshots payload: {:?}", payload);
            })?;
            Some(Self {
                camera_label,
                state,
            })
        } else {
            None
        }
    }
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
struct RecordingsState {
    camera_label: String,
    state: bool,
}

impl RecordingsState {
    #[must_use]
    pub fn from_topic_parts(topic_parts: &[&str], payload: &bytes::Bytes) -> Option<Self> {
        if topic_parts.len() > 3 && topic_parts[2] == "recordings" && topic_parts[3] == "state" {
            let camera_label = topic_parts[1].to_string();
            let state = on_off_from_bytes(payload.to_vec()).tap_none(|| {
                tracing::error!("Failed to parse snapshots payload: {:?}", payload);
            })?;
            Some(Self {
                camera_label,
                state,
            })
        } else {
            None
        }
    }
}

#[must_use]
#[derive(Debug)]
pub enum CapturedPayloads {
    CameraRecordingsState(RecordingsState),
    CameraSnapshotsState(SnapshotsState),
    Snapshot(image::DynamicImage),
}

fn on_off_from_bytes(value: Vec<u8>) -> Option<bool> {
    let value = String::from_utf8(value).ok()?;
    let value = value.trim();
    if value == "ON" {
        Some(true)
    } else if value == "OFF" {
        Some(false)
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use rstest::rstest;
    use test_utils::random::{
        Seed, make_random_alphanumeric_string, make_seedable_rng, random_seed,
    };

    use super::*;

    #[rstest]
    #[trace]
    #[case(b"ON".to_vec(), Some(true))]
    #[trace]
    #[case(b"OFF".to_vec(), Some(false))]
    #[trace]
    #[case(b"abcdefg".to_vec(), None)]
    fn snapshots_state(
        random_seed: Seed,
        #[case] payload: Vec<u8>,
        #[case] expected_state: Option<bool>,
    ) {
        let mut rng = make_seedable_rng(random_seed);

        let mqtt_topic_prefix = make_random_alphanumeric_string(&mut rng, 20);

        let mut config = VideoSyncConfig::default();

        config.set_mqtt_frigate_topic_prefix(Some(mqtt_topic_prefix.clone()));

        {
            let camera_name = make_random_alphanumeric_string(&mut rng, 20);

            let parse_result = CapturedPayloads::from_publish(
                &config,
                &format!("{mqtt_topic_prefix}/{camera_name}/snapshots/state"),
                &Bytes::from_owner(payload),
            );

            if let Some(expected_state) = expected_state {
                let parse_result = parse_result.unwrap();

                assert_eq!(
                    parse_result.into_snapshots_state().unwrap(),
                    SnapshotsState {
                        camera_label: camera_name,
                        state: expected_state
                    }
                );
            } else {
                assert!(parse_result.is_none())
            }
        }
    }

    #[rstest]
    #[trace]
    #[case(b"ON".to_vec(), Some(true))]
    #[trace]
    #[case(b"OFF".to_vec(), Some(false))]
    #[trace]
    #[case(b"abcdefg".to_vec(), None)]
    fn recordings_state(
        random_seed: Seed,
        #[case] payload: Vec<u8>,
        #[case] expected_state: Option<bool>,
    ) {
        let mut rng = make_seedable_rng(random_seed);

        let mqtt_topic_prefix = make_random_alphanumeric_string(&mut rng, 20);

        let mut config = VideoSyncConfig::default();

        config.set_mqtt_frigate_topic_prefix(Some(mqtt_topic_prefix.clone()));

        {
            let camera_name = make_random_alphanumeric_string(&mut rng, 20);

            let parse_result = CapturedPayloads::from_publish(
                &config,
                &format!("{mqtt_topic_prefix}/{camera_name}/recordings/state"),
                &Bytes::from_owner(payload),
            );

            if let Some(expected_state) = expected_state {
                let parse_result = parse_result.unwrap();

                assert_eq!(
                    parse_result.into_recordings_state().unwrap(),
                    RecordingsState {
                        camera_label: camera_name,
                        state: expected_state
                    }
                );
            } else {
                assert!(parse_result.is_none())
            }
        }
    }
}
