use tap::TapOptional;

use super::utils::on_off_from_bytes;

#[must_use]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RecordingsState {
    pub camera_label: String,
    pub state: bool,
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
    #[trace]
    fn recordings_state(
        random_seed: Seed,
        #[case] payload: Vec<u8>,
        #[case] expected_state: Option<bool>,
    ) {
        use crate::{config::MqttHandlerConfig, types::CapturedPayloads};

        let mut rng = make_seedable_rng(random_seed);

        let mqtt_topic_prefix = make_random_alphanumeric_string(&mut rng, 20);

        let mut config = MqttHandlerConfig::default();

        config.mqtt_frigate_topic_prefix = mqtt_topic_prefix.clone();

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
