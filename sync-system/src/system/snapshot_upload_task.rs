use super::{
    FileSenderMaker, MAX_ATTEMPT_COUNT,
    common::{make_file_senders, split_file_senders_and_descriptors},
};
use crate::{config::PathDescriptors, system::SLEEP_AFTER_ERROR};
use mqtt_handler::types::snapshot::Snapshot;
use std::{path::Path, sync::Arc};
use tokio::task::JoinHandle;

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

    async fn launch_inner(self) {
        {
            // Take a copy of all the descriptors as the initial ones to use for the upload
            let mut remaining_descriptors = self
                .file_senders_path_descriptors
                .path_descriptors
                .as_ref()
                .clone();

            let file_sender_maker = self.file_sender_maker.clone();

            for attempt_number in 0..MAX_ATTEMPT_COUNT {
                if remaining_descriptors.is_empty() {
                    // no +1 here because it finished in last iter
                    tracing::info!(
                        "Done uploading snapshot at attempt '{attempt_number}' for camera {}",
                        self.snapshot.camera_label
                    );
                    break;
                }

                let file_senders =
                    make_file_senders(&file_sender_maker, &remaining_descriptors).await;
                let (file_senders, path_descriptors) =
                    split_file_senders_and_descriptors(file_senders);

                // The descriptors that we failed to open, are the ones we'll attempt open again in the next iteration
                remaining_descriptors = path_descriptors;

                for s in &file_senders {
                    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let dir = Path::new(&date);
                    let upload_path = dir.join(self.snapshot.make_file_name());

                    match s.as_ref().mkdir_p(dir).await.and(
                        s.as_ref()
                            .put_from_memory(&self.snapshot.image_bytes, &upload_path)
                            .await,
                    ) {
                        Ok(()) => {
                            tracing::info!(
                                "Successfully uploaded snapshot {} to {} at attempt {}",
                                upload_path.display(),
                                s.path_descriptor(),
                                attempt_number + 1, // Counting starts from 1
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Error uploading snapshot {} to {}. Attempt number: {}. Error: {e}",
                                upload_path.display(),
                                s.path_descriptor(),
                                attempt_number + 1, // Counting starts from 1
                            );

                            // Since it failed, we try again later
                            remaining_descriptors.push(s.path_descriptor().clone());
                            tokio::time::sleep(SLEEP_AFTER_ERROR).await;
                        }
                    }
                }
            }

            if remaining_descriptors.is_empty() {
                tracing::debug!(
                    "Success: Reaching the end of snapshot upload code for camera {}",
                    self.snapshot.camera_label
                );
            } else {
                tracing::debug!(
                    "Error: Reaching the end of snapshot upload code for camera {} with {} destination(s) having received the snapshot. These are: '{}'",
                    self.snapshot.camera_label,
                    remaining_descriptors.len(),
                    remaining_descriptors
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
    }

    pub fn launch(self) -> JoinHandle<()> {
        tokio::task::spawn(self.launch_inner())
    }
}
