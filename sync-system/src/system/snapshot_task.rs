use std::{path::Path, sync::Arc};

use file_sender::path_descriptor::PathDescriptor;
use mqtt_handler::types::snapshot::Snapshot;
use tokio::task::JoinHandle;

use crate::{config::PathDescriptors, system::SLEEP_AFTER_ERROR};

use super::{
    FileSenderMaker, MAX_ATTEMPT_COUNT,
    common::{FileSenderOrPathDescriptor, split_file_senders_and_descriptors},
};

#[must_use]
pub struct SnapshotTask<S> {
    snapshot: Snapshot,
    file_sender_maker: Arc<S>,
    file_senders_path_descriptors: PathDescriptors,
}

impl<S: FileSenderMaker> SnapshotTask<S> {
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

    pub fn launch(self) -> JoinHandle<()> {
        tokio::task::spawn(async move {
            // Take a copy of all the descriptors as the initial ones to use for the upload
            let mut remaining_descriptors: Vec<Arc<PathDescriptor>> = self
                .file_senders_path_descriptors
                .path_descriptors
                .as_ref()
                .clone();

            for attempt_number in 0..MAX_ATTEMPT_COUNT {
                if remaining_descriptors.is_empty() {
                    // no +1 here because it finished in last iter
                    tracing::info!(
                        "Done uploading snapshot at attempt '{attempt_number}' for camera {}",
                        self.snapshot.camera_label
                    );
                    break;
                }

                let file_senders = self.make_file_senders(&remaining_descriptors);
                let (file_senders, path_descriptors) =
                    split_file_senders_and_descriptors(file_senders);

                // The descriptors that we failed to open, are the ones we'll open in the next iteration
                remaining_descriptors = path_descriptors;

                for s in &file_senders {
                    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let dir = Path::new(&date);
                    let upload_path = dir.join(self.snapshot.make_file_name());

                    match s.as_ref().mkdir_p(dir).and(
                        s.as_ref()
                            .put_from_memory(&self.snapshot.image_bytes, &upload_path),
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

            tracing::debug!(
                "Reaching the end of snapshot upload code for camera {}",
                self.snapshot.camera_label
            );
        })
    }

    fn make_file_senders(
        &self,
        remaining_path_descriptors: &[Arc<PathDescriptor>],
    ) -> Vec<FileSenderOrPathDescriptor> {
        let senders = remaining_path_descriptors
            .iter()
            .map(|d| (d.clone(), (self.file_sender_maker)(d)))
            .collect::<Vec<_>>();

        let mut result = Vec::new();

        for (descriptor, sender_result) in senders {
            match sender_result {
                Ok(s) => {
                    result.push(s.into());
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to create file sender with descriptor `{descriptor}`: {e}",
                    );
                    result.push(descriptor.into());
                }
            }
        }

        result
    }
}
