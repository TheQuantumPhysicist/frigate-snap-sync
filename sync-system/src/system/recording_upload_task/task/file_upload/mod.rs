mod review_with_clip;

use crate::{
    config::PathDescriptors,
    system::{
        common::file_upload::upload_file,
        traits::{FileSenderMaker, FrigateApiMaker},
    },
};
use anyhow::Context;
use frigate_api_caller::{config::FrigateApiConfig, traits::FrigateApi};
use mqtt_handler::types::reviews::ReviewProps;
use review_with_clip::ReviewWithClip;
use std::{path::PathBuf, sync::Arc};
use utils::time_getter::TimeGetter;

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ReviewUploadError {
    #[error("Frigate API construction failed with error: {0}")]
    APIConstructionFailed(String),
    #[error("Retrieving clip returned an error: {0}")]
    ClipRetrievalError(String),
    #[error(
        "Retrieving video from API returned an empty video. This is an unrecoverable error. Id: "
    )]
    EmptyVideoReturned(String),
    #[error("Review recording upload failed: {0}")]
    RecordingUpload(String),
}

#[must_use]
pub struct ReviewUpload<F, S> {
    review: Arc<dyn ReviewProps>,
    state: ReviewUploadState,
    /// When uploading, we can upload the same review in two different names.
    /// This is because we want to keep the latest available version of the
    /// video without deleting it while we upload the next video. So every
    /// upload of the same review, can add more on the previous one. This
    /// helps in case the connection is lost, the most amount of information
    /// is left.
    alternative_upload: bool,

    frigate_api_config: Arc<FrigateApiConfig>,
    frigate_api_maker: Arc<F>,
    file_sender_maker: Arc<S>,
    time_getter: TimeGetter,
    path_descriptors: PathDescriptors,
}

impl<F, S> ReviewUpload<F, S>
where
    F: FrigateApiMaker,
    S: FileSenderMaker,
{
    pub fn new(
        review: Arc<dyn ReviewProps>,
        alternative_upload: bool,
        frigate_api_config: Arc<FrigateApiConfig>,
        frigate_api_maker: Arc<F>,
        file_sender_maker: Arc<S>,
        path_descriptors: PathDescriptors,
    ) -> Self {
        Self {
            review,
            state: ReviewUploadState::default(),
            alternative_upload,
            frigate_api_config,
            frigate_api_maker,
            file_sender_maker,
            time_getter: TimeGetter::default(),
            path_descriptors,
        }
    }

    pub async fn run(&mut self) -> Result<(), ReviewUploadError> // The result indicates whether all the steps have finished successfully for the file, since review files is uploaded sequentially
    {
        let id = self.review.id().to_string();

        loop {
            match &self.state {
                ReviewUploadState::Start => self.state = ReviewUploadState::GettingVideoFromAPI,
                ReviewUploadState::GettingVideoFromAPI => {
                    let api = self
                        .make_frigate_api()
                        .map_err(|e| ReviewUploadError::APIConstructionFailed(e.to_string()))?;

                    let start_ts = self.review.start_time();
                    let end_ts = self
                        .review
                        .end_time()
                        .unwrap_or(self.time_getter.get_time().as_unix_timestamp_f64());

                    let clip = api
                        .recording_clip(self.review.camera_name(), start_ts, end_ts)
                        .await
                        .context("Retrieving video clip failed")
                        .map_err(|e| ReviewUploadError::ClipRetrievalError(e.to_string()))?;

                    let Some(clip) = clip else {
                        return Err(ReviewUploadError::EmptyVideoReturned(id));
                    };

                    let review_with_clip =
                        ReviewWithClip::new(self.review.clone(), clip, self.alternative_upload);

                    self.state = ReviewUploadState::UploadToStore(review_with_clip);
                }
                ReviewUploadState::UploadToStore(rec) => {
                    upload_file(
                        rec,
                        self.path_descriptors.path_descriptors.as_ref().clone(),
                        self.file_sender_maker.clone(),
                        3,
                    )
                    .await
                    .map_err(|e| ReviewUploadError::RecordingUpload(e.to_string()))?;

                    {
                        // rec.
                    }

                    self.state = ReviewUploadState::DeleteTheAlternative(rec.alternative_path());
                }
                ReviewUploadState::DeleteTheAlternative(_alt_path) => {
                    // TODO: implement this if exists
                    // let file_senders = make_file_senders(&self.file_sender_maker, &remaining_descriptors).await;

                    // let path = self.file_sender_maker

                    self.state = ReviewUploadState::Done;
                }
                ReviewUploadState::Done => return Ok(()),
            }
        }
    }

    pub fn make_frigate_api(&self) -> anyhow::Result<Arc<dyn FrigateApi>> {
        (self.frigate_api_maker)(&self.frigate_api_config)
    }
}

#[derive(Debug, Clone, Default)]
pub enum ReviewUploadState {
    #[default]
    Start,
    GettingVideoFromAPI,
    UploadToStore(ReviewWithClip),
    DeleteTheAlternative(PathBuf),
    Done,
}

#[cfg(test)]
mod tests;
