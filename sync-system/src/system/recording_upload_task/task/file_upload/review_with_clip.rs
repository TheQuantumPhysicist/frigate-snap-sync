use crate::system::common::file_upload::UploadableFile;
use mqtt_handler::types::reviews::ReviewProps;
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone)]
pub struct ReviewWithClip {
    review: Arc<dyn ReviewProps>,
    clip: Vec<u8>,
    alternative_upload: bool,
}

impl ReviewWithClip {
    pub fn new(review: Arc<dyn ReviewProps>, clip: Vec<u8>, alternative_upload: bool) -> Self {
        Self {
            review,
            clip,
            alternative_upload,
        }
    }

    fn alternative_name_suffix(&self) -> &str {
        if self.alternative_upload { "-1" } else { "-0" }
    }
}

impl UploadableFile for ReviewWithClip {
    fn file_bytes(&self) -> &[u8] {
        &self.clip
    }

    fn file_name(&self) -> std::path::PathBuf {
        let datetime = chrono::Local::now()
            .format("%Y-%m-%d_%H-%M-%S%z")
            .to_string();
        format!(
            "RecordingClip-{}-{datetime}{}.jpg",
            self.review.camera_name(),
            self.alternative_name_suffix()
        )
        .into()
    }

    fn upload_dir(&self) -> std::path::PathBuf {
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        PathBuf::from(date)
    }

    fn file_description(&self) -> String {
        format!("Recording clip with id {}", self.review.id())
    }
}
