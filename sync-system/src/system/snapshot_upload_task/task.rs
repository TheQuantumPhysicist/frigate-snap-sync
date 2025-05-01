use crate::{
    config::PathDescriptors,
    system::{
        common::file_upload::{RemoteFileOp, UploadableFile, remote_file_op},
        traits::FileSenderMaker,
    },
};
use mqtt_handler::types::snapshot::Snapshot;
use std::{path::PathBuf, sync::Arc};
use utils::time::Time;

const MAX_ATTEMPT_COUNT: u32 = 128;

#[must_use]
pub struct SnapshotUploadTask<S> {
    snapshot: Arc<dyn UploadableFile>,
    file_sender_maker: Arc<S>,
    file_senders_path_descriptors: PathDescriptors,
}

impl<S: FileSenderMaker> SnapshotUploadTask<S> {
    pub fn new(
        snapshot: Arc<dyn UploadableFile>,
        file_sender_maker: Arc<S>,
        file_senders_path_descriptors: PathDescriptors,
    ) -> Self {
        Self {
            snapshot,
            file_sender_maker,
            file_senders_path_descriptors,
        }
    }

    pub async fn run(self) {
        let snapshot = self.snapshot;
        let path_descriptors = self
            .file_senders_path_descriptors
            .path_descriptors
            .as_ref()
            .clone();
        let file_sender_maker = self.file_sender_maker;

        let _ = remote_file_op(
            RemoteFileOp::Upload(snapshot.as_ref()),
            path_descriptors,
            file_sender_maker,
            MAX_ATTEMPT_COUNT,
        )
        .await
        .inspect_err(|e| tracing::error!("Snapshot remote op file error: {e}"));
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
        let date = Time::local_time_in_dir_foramt();
        PathBuf::from(date)
    }

    fn file_description(&self) -> String {
        format!("Snapshot from camera {}", self.camera_label)
    }
}
