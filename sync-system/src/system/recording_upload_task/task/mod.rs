mod file_upload;

use crate::{
    config::PathDescriptors,
    system::traits::{FileSenderMaker, FrigateApiMaker},
};
use file_upload::ReviewUpload;
use frigate_api_caller::config::FrigateApiConfig;
use mqtt_handler::types::reviews::{self, ReviewProps};
use std::sync::Arc;
use tokio::sync::oneshot;
use utils::time_getter::TimeGetter;

const DEFAULT_RETRY_PERIOD: std::time::Duration = std::time::Duration::from_secs(60);
const DEFAULT_MAX_RETRY_ATTEMPTS: u32 = 60;

type ReviewsReceiver =
    tokio::sync::mpsc::UnboundedReceiver<(Arc<dyn ReviewProps>, Option<oneshot::Sender<()>>)>;

/// A struct that tracks the updates of a single review, and keeps uploading until
/// the review type "end" has been reached, or a deadline is hit.
/// On every update, the upload will trigger again.
#[must_use]
pub struct SingleRecordingUploadTask<F, S> {
    /// The current review that is being processed for upload
    current_review: Arc<dyn ReviewProps>,

    /// The first task sent, if provided, has to be resolved for this to receive a send,
    /// after having processed the very first review and the event loop has started.
    /// This is an Option because it can be used only once, and then it's empty
    first_review_resolved_sender: Option<oneshot::Sender<()>>,

    /// Updates about the this review is received through this channel
    /// The oneshot sends after processing the review finishes
    reviews_receiver: ReviewsReceiver,

    /// A send will happen to this, if exists, when the event loop shuts down
    end_review_resolved_sender: Option<oneshot::Sender<UploadConclusion>>,

    frigate_api_config: Arc<FrigateApiConfig>,
    frigate_api_maker: Arc<F>,
    file_sender_maker: Arc<S>,
    path_descriptors: PathDescriptors,

    /// The upload that is currently running. This can be replaced by a new
    /// object when an update is received.
    current_upload_process: Option<ReviewUpload<F, S>>,

    // See `ReviewUpload` for more information.
    alternative_upload: bool,

    retry_attempt: u32,
    max_retry_attempts: u32,

    retry_duration: std::time::Duration,

    time_getter: TimeGetter,
}

impl<F, S> SingleRecordingUploadTask<F, S>
where
    F: FrigateApiMaker,
    S: FileSenderMaker,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        start_review: Arc<dyn ReviewProps>,
        first_review_resolved_sender: Option<oneshot::Sender<()>>,
        reviews_receiver: ReviewsReceiver,
        end_review_resolved_sender: Option<oneshot::Sender<UploadConclusion>>,

        frigate_api_config: Arc<FrigateApiConfig>,
        frigate_api_maker: Arc<F>,
        file_sender_maker: Arc<S>,
        path_descriptors: PathDescriptors,
        max_retry_attempts: Option<u32>,
        retry_period: Option<std::time::Duration>,
        time_getter: TimeGetter,
    ) -> Self {
        Self {
            current_review: start_review, // The current one is the start one
            first_review_resolved_sender,
            end_review_resolved_sender,

            reviews_receiver,

            frigate_api_config,
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,

            alternative_upload: false,

            current_upload_process: None,

            retry_attempt: 0,
            max_retry_attempts: max_retry_attempts.unwrap_or(DEFAULT_MAX_RETRY_ATTEMPTS),

            retry_duration: retry_period.unwrap_or(DEFAULT_RETRY_PERIOD),

            time_getter,
        }
    }

    pub async fn start(mut self) -> String {
        let id = self.current_review.id().to_string();

        tracing::debug!("Launched recoding upload task for review with id: {id}");

        // We have the initial review, so we use it
        let _ = self.on_received_review(self.current_review.clone()).await;
        self.first_review_resolved_sender
            .take()
            .expect("Since this is running once, it must exist")
            .send(())
            .expect("The channel must exist");

        let mut final_result = UploadConclusion::NotDone;

        loop {
            let retry_instant = tokio::time::Instant::now() + self.retry_duration;

            tokio::select! {
                Some((review, result_sender)) = self.reviews_receiver.recv() => {
                    final_result = self.on_received_review(review).await;

                    if let Some(sender) = result_sender {
                        if sender.send(()).is_err() {
                            tracing::error!("CRITICAL: Signal that confirms the result of uploading a recording is dead.
                                This can indicate a race and bad programming. Should never happen.");
                        }
                    }

                    match final_result {
                        UploadConclusion::Done => break,
                        UploadConclusion::NotDone => self.retry_attempt += 1,
                    };

                }

                () = tokio::time::sleep_until(retry_instant) => {
                    if self.retry_attempt >= self.max_retry_attempts {
                        tracing::error!(
                            "Upload cancelled for review recording with id `{id}` after having retried {} times.", self.retry_attempt
                        );
                        break;
                    }

                    tracing::debug!("Retrying to upload recording with id `{id}` after having waited: {}", humantime::format_duration(self.retry_duration));
                    final_result = self.run_upload().await;

                    match final_result {
                        UploadConclusion::Done => break,
                        UploadConclusion::NotDone => self.retry_attempt += 1,
                    }
                }
            }
        }

        if let Some(sender) = self.end_review_resolved_sender {
            if sender.send(final_result).is_err() {
                tracing::error!(
                    "CRITICAL: Sender that indicates that a single recording upload task is done failed to indicate that the event loop is done. This indicates a race."
                );
            }
        }

        id
    }

    pub async fn on_received_review(&mut self, review: Arc<dyn ReviewProps>) -> UploadConclusion {
        self.current_review = review.clone();

        let new_upload_process = ReviewUpload::new(
            review,
            self.alternative_upload,
            self.frigate_api_config.clone(),
            self.frigate_api_maker.clone(),
            self.file_sender_maker.clone(),
            self.path_descriptors.clone(),
            self.time_getter.clone(),
        );

        // Previous upload attempts will be be cancelled if a new recording has arrived.
        // The cancellation happens because this task is not meant to be concurrent
        // (the previous upload process object will be destroyed).
        self.current_upload_process = Some(new_upload_process);

        self.run_upload().await
    }

    async fn run_upload(&mut self) -> UploadConclusion {
        let Some(current_upload_process) = self.current_upload_process.as_mut() else {
            // Once an upload has been initiated, this can never be None again
            tracing::error!("CRITICAL: INVARIANT BROKEN: Current upload process is empty");
            return UploadConclusion::NotDone;
        };

        let result = current_upload_process.run().await;

        match result {
            Ok(()) => {
                // When an upload is successful, the next upload will go to the alternative file name
                self.alternative_upload = !self.alternative_upload;

                if self.current_review.type_field() == reviews::payload::TypeField::End {
                    UploadConclusion::Done
                } else {
                    UploadConclusion::NotDone
                }
            }
            Err(e) => {
                tracing::error!("Recording upload finished with error: {}", e);
                UploadConclusion::NotDone
            }
        }
    }
}

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadConclusion {
    NotDone,
    Done,
}

#[cfg(test)]
mod tests;
