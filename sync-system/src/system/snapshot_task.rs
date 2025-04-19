use std::{marker::PhantomData, path::Path, sync::Arc};

use file_sender::path_descriptor::PathDescriptor;
use futures::future::join_all;
use mqtt_handler::types::snapshot::Snapshot;
use tokio::task::JoinHandle;

use crate::{config::PathDescriptors, system::SLEEP_AFTER_ERROR};

use super::{
    FileSenderMaker, MAX_ATTEMPT_COUNT,
    common::{FileSenderOrPathDescriptor, split_file_senders_and_descriptors},
    traits::AsyncFileSenderResult,
};

#[must_use]
pub struct SnapshotTask<S, F> {
    snapshot: Snapshot,
    file_sender_maker: Arc<S>,
    file_senders_path_descriptors: PathDescriptors,
    _marker: PhantomData<F>,
}

impl<F: AsyncFileSenderResult, S: FileSenderMaker<F>> SnapshotTask<S, F> {
    pub fn new(
        snapshot: Snapshot,
        file_sender_maker: Arc<S>,
        file_senders_path_descriptors: PathDescriptors,
    ) -> Self {
        Self {
            snapshot,
            file_sender_maker,
            file_senders_path_descriptors,
            _marker: PhantomData,
        }
    }

    async fn launch_inner(self) {
        {
            // Take a copy of all the descriptors as the initial ones to use for the upload
            let remaining_descriptors = self
                .file_senders_path_descriptors
                .path_descriptors
                .as_ref()
                .clone();

            let file_sender_maker = self.file_sender_maker.clone();

            let remaining_descriptors = tokio::sync::Mutex::new(remaining_descriptors);

            for attempt_number in 0..MAX_ATTEMPT_COUNT {
                if remaining_descriptors.lock().await.is_empty() {
                    // no +1 here because it finished in last iter
                    tracing::info!(
                        "Done uploading snapshot at attempt '{attempt_number}' for camera {}",
                        self.snapshot.camera_label
                    );
                    break;
                }

                let file_senders =
                    Self::make_file_senders(file_sender_maker.clone(), &remaining_descriptors)
                        .await;
                let (file_senders, path_descriptors) =
                    split_file_senders_and_descriptors(file_senders);

                // The descriptors that we failed to open, are the ones we'll open in the next iteration
                *remaining_descriptors.lock().await = path_descriptors;

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
                            remaining_descriptors
                                .lock()
                                .await
                                .push(s.path_descriptor().clone());
                            tokio::time::sleep(SLEEP_AFTER_ERROR).await;
                        }
                    }
                }
            }

            tracing::debug!(
                "Reaching the end of snapshot upload code for camera {}",
                self.snapshot.camera_label
            );
        }
    }

    pub fn launch(self) -> JoinHandle<()> {
        tokio::task::spawn(self.launch_inner())
    }

    async fn make_file_senders(
        file_sender_maker: Arc<S>,
        remaining_path_descriptors: &tokio::sync::Mutex<Vec<Arc<PathDescriptor>>>,
    ) -> Vec<FileSenderOrPathDescriptor> {
        let ds = remaining_path_descriptors.lock().await;
        let senders = ds
            .iter()
            .map(|d| (d.clone(), (file_sender_maker)(d.clone())));

        let senders_vec = join_all(senders.map(|(d, s)| async { (d, s.await) })).await;

        senders_vec
            .into_iter()
            .map(|(descriptor, sender_result)| match sender_result {
                Ok(s) => s.into(),
                Err(e) => {
                    tracing::error!(
                        "Failed to create file sender with descriptor `{descriptor}`: {e}",
                    );
                    descriptor.into()
                }
            })
            .collect::<Vec<_>>()
    }
}
