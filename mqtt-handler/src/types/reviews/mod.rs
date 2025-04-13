use payload::ReviewsPayload;

pub mod payload;

#[derive(Debug, Clone)]
pub struct Reviews {
    pub payload: payload::ReviewsPayload,
}

impl Reviews {
    #[must_use]
    pub fn from_topic_parts(topic_parts: &[&str], payload: &bytes::Bytes) -> Option<Self> {
        // <prefix>/reviews
        if topic_parts.len() == 2 && topic_parts[1] == "reviews" {
            let payload_str = match String::from_utf8(payload.to_vec()) {
                Ok(payload_str) => payload_str,
                Err(e) => {
                    tracing::error!(
                        "Parsing a review payload failed. Will attempt a lossy read: `{e}`",
                    );
                    String::from_utf8_lossy(payload).to_string()
                }
            };

            let payload = match serde_json::from_str::<ReviewsPayload>(&payload_str) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Parsing payload to json failed: `{e}`.");
                    return None;
                }
            };

            Some(Self { payload })
        } else {
            None
        }
    }
}
