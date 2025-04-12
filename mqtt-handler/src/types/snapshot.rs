#[must_use]
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub snapshot_image: image::DynamicImage,
    pub camera_label: String,
    pub object_name: String,
}

impl Snapshot {
    #[must_use]
    pub fn from_topic_parts(topic_parts: &[&str], payload: &bytes::Bytes) -> Option<Self> {
        // <prefix>/<camera_name>/<object_name>/snapshot
        if topic_parts.len() > 3 && topic_parts[3] == "snapshot" {
            let camera_label = topic_parts[1].to_string();
            let object_name = topic_parts[2].to_string();
            let snapshot_image =
                match image::load_from_memory_with_format(payload, image::ImageFormat::Jpeg) {
                    Ok(img) => img,
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse `snapshot` topic (${}) image with error: `{e}`",
                            topic_parts.join("/")
                        );
                        return None;
                    }
                };
            Some(Self {
                snapshot_image,
                camera_label,
                object_name,
            })
        } else {
            None
        }
    }
}
