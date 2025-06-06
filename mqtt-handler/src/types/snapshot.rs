use std::path::PathBuf;

#[must_use]
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub image_bytes: Vec<u8>, // a raw copy of the image, to save it to disk
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
            let _snapshot_image =
                match image::load_from_memory_with_format(payload, image::ImageFormat::Jpeg) {
                    Ok(img) => img,
                    Err(e) => {
                        tracing::error!(
                            "Failed to parse `snapshot` topic (${}) image with error: `{e}`",
                            topic_parts.join("/")
                        );
                        return None;
                    }
                };
            Some(Self {
                image_bytes: payload.to_vec(),
                camera_label,
                object_name,
            })
        } else {
            None
        }
    }

    #[must_use]
    pub fn make_file_name(&self) -> PathBuf {
        let datetime = chrono::Local::now()
            .format("%Y-%m-%d_%H-%M-%S%z")
            .to_string();
        format!(
            "Snapshot-{}-{datetime}-{}.jpg",
            self.camera_label, self.object_name
        )
        .into()
    }
}
