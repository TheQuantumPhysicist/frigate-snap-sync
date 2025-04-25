use crate::system::traits::FileSenderMaker;
use file_sender::{path_descriptor::PathDescriptor, traits::StoreDestination};
use std::{path::PathBuf, sync::Arc};

use super::file_senders::{make_file_senders, split_file_senders_and_descriptors};

const SLEEP_AFTER_ERROR: std::time::Duration = std::time::Duration::from_secs(5);

pub trait UploadableFile {
    fn file_bytes(&self) -> &[u8];
    fn file_name(&self) -> PathBuf;
    fn file_description(&self) -> String;
    fn upload_dir(&self) -> PathBuf;
    fn full_upload_path(&self) -> PathBuf {
        self.upload_dir().join(self.file_name())
    }
}

// TODO: separate upload and other possible ops (like deleting a file) so that we can reuse the multiple file-senders algorithm
pub async fn upload_file<S: FileSenderMaker>(
    file: &impl UploadableFile,
    path_descriptors: Vec<Arc<PathDescriptor>>,
    file_sender_maker: Arc<S>,
    max_attempt_count: u32,
) -> anyhow::Result<()> {
    // Take a copy of all the descriptors as the initial ones to use for the upload
    let mut remaining_descriptors = path_descriptors;

    for attempt_number in 0..max_attempt_count {
        if remaining_descriptors.is_empty() {
            // no +1 here because it finished in last iter
            tracing::info!(
                "Done uploading file at attempt '{attempt_number}' for: {}",
                file.file_description()
            );
            break;
        }

        let file_senders = make_file_senders(&file_sender_maker, &remaining_descriptors).await;
        let (file_senders, path_descriptors) = split_file_senders_and_descriptors(file_senders);

        // The descriptors that we failed to open, are the ones we'll attempt open again in the next iteration
        remaining_descriptors = path_descriptors;

        for s in &file_senders {
            let op_result = upload_file_inner(file, s, attempt_number).await;
            if op_result.is_err() {
                // Since it failed, we try again later
                remaining_descriptors.push(s.path_descriptor().clone());
                tokio::time::sleep(SLEEP_AFTER_ERROR).await;
            }
        }
    }

    if remaining_descriptors.is_empty() {
        tracing::debug!(
            "Success: Reaching the end of file upload code for camera {}",
            file.file_description()
        );

        Ok(())
    } else {
        let error = format!(
            "Error: Reaching the end of file upload code for file `{}` with {} destination(s) having received the file. These are: '{}'",
            file.file_description(),
            remaining_descriptors.len(),
            remaining_descriptors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );

        Err(anyhow::anyhow!("{error}"))
    }
}

async fn upload_file_inner(
    file: &impl UploadableFile,
    file_sender: &Arc<dyn StoreDestination<Error = anyhow::Error>>,
    attempt_number: u32,
) -> anyhow::Result<()> {
    let dir = file.upload_dir();
    let upload_path = file.full_upload_path();

    let result = file_sender.as_ref().mkdir_p(&dir).await.and(
        file_sender
            .as_ref()
            .put_from_memory(file.file_bytes(), &upload_path)
            .await,
    );

    match &result {
        Ok(()) => {
            tracing::info!(
                "Successfully uploaded file {} to {} at attempt {}",
                upload_path.display(),
                file_sender.path_descriptor(),
                attempt_number + 1, // Counting starts from 1
            );
        }
        Err(e) => {
            tracing::error!(
                "Error uploading file {} to {}. Attempt number: {}. Error: {e}",
                upload_path.display(),
                file_sender.path_descriptor(),
                attempt_number + 1, // Counting starts from 1
            );
        }
    }

    result
}
