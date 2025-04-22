use super::{
    FileSenderMaker,
    common::file_upload::{UploadableFile, upload_file},
};
use crate::config::PathDescriptors;
use file_sender::path_descriptor::PathDescriptor;
use mqtt_handler::types::snapshot::Snapshot;
use std::{path::PathBuf, sync::Arc};
use tokio::task::JoinHandle;

const MAX_ATTEMPT_COUNT: u32 = 128;

#[must_use]
pub struct SnapshotUploadTask<S> {
    snapshot: Snapshot,
    file_sender_maker: Arc<S>,
    file_senders_path_descriptors: PathDescriptors,
}

impl<S: FileSenderMaker> SnapshotUploadTask<S> {
    pub fn new(
        snapshot: Snapshot,
        file_sender_maker: Arc<S>,
        file_senders_path_descriptors: PathDescriptors,
    ) -> Self {
        Self {
            snapshot,
            file_sender_maker,
            file_senders_path_descriptors,
        }
    }

    async fn launch_inner(
        file: impl UploadableFile,
        path_descriptors: Vec<Arc<PathDescriptor>>,
        file_sender_maker: Arc<S>,
    ) {
        let _ = upload_file(
            &file,
            path_descriptors,
            file_sender_maker,
            MAX_ATTEMPT_COUNT,
        )
        .await
        .inspect_err(|e| tracing::error!("{e}"));
    }

    pub fn launch(self) -> JoinHandle<()> {
        let snapshot = self.snapshot;
        let path_descriptors = self
            .file_senders_path_descriptors
            .path_descriptors
            .as_ref()
            .clone();
        let file_sender_maker = self.file_sender_maker;

        tokio::task::spawn(Self::launch_inner(
            snapshot,
            path_descriptors,
            file_sender_maker,
        ))
    }
}

impl UploadableFile for Snapshot {
    fn file_bytes(&self) -> &[u8] {
        &self.image_bytes
    }

    fn file_name(&self) -> PathBuf {
        self.make_file_name()
    }

    fn upload_dir(&self) -> PathBuf {
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        PathBuf::from(date)
    }

    fn file_description(&self) -> String {
        format!("Snapshot from camera {}", self.camera_label)
    }
}
