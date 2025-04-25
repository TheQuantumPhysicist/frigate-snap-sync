use crate::system::common::file_upload::UploadableFile;
use mqtt_handler::types::reviews::ReviewProps;
use std::{path::PathBuf, sync::Arc};
use utils::time::Time;

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

    /// To facilitate upload two different files in an alternating fashion, such that,
    /// we have at least one complete file in the store,
    /// and only delete the other file (alternative) when the first is successful.
    /// This function returns two possible suffixes for the file name.
    fn alternative_name_suffix(&self, flip: bool) -> &str {
        #[allow(clippy::if_not_else)]
        if self.alternative_upload != flip
        // We use '!= flip' as an XOR operation that flips the boolean
        {
            "-1"
        } else {
            "-0"
        }
    }

    /// The alternative path to the current setting.
    /// We use this to delete this file when the first upload is complete.
    pub fn alternative_path(&self) -> PathBuf {
        let datetime = chrono::Local::now()
            .format("%Y-%m-%d_%H-%M-%S%z")
            .to_string();
        format!(
            "RecordingClip-{}-{datetime}{}.mp4",
            self.review.camera_name(),
            self.alternative_name_suffix(true)
        )
        .into()
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
            "RecordingClip-{}-{datetime}{}.mp4",
            self.review.camera_name(),
            self.alternative_name_suffix(false)
        )
        .into()
    }

    fn upload_dir(&self) -> std::path::PathBuf {
        // TODO: get the date from the review, not from current time
        let date = Time::local_time_in_dir_foramt();
        PathBuf::from(date)
    }

    fn file_description(&self) -> String {
        format!("Recording clip with id {}", self.review.id())
    }
}
