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
use std::sync::Arc;
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
    Done,
}

#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Arc};

    use crate::config::PathDescriptors;

    use super::ReviewUpload;
    use file_sender::{
        make_inmemory_filesystem, path_descriptor::PathDescriptor, traits::StoreDestination,
    };
    use frigate_api_caller::{config::FrigateApiConfig, traits::FrigateApi};
    use mocks::{frigate_api::make_frigate_client_mock, store_dest::make_store_mock};
    use mqtt_handler::types::reviews::{ReviewProps, payload};

    #[derive(Debug, Clone)]
    struct TestReviewData {
        camera_name: String,
        start_time: f64,
        end_time: f64,
        id: String,
        type_field: payload::TypeField,
    }

    impl ReviewProps for TestReviewData {
        fn camera_name(&self) -> &str {
            &self.camera_name
        }

        fn id(&self) -> &str {
            &self.id
        }

        fn start_time(&self) -> f64 {
            self.start_time
        }

        fn end_time(&self) -> Option<f64> {
            Some(self.end_time)
        }

        fn type_field(&self) -> payload::TypeField {
            self.type_field
        }
    }

    #[tokio::test]
    async fn basic_upload_in_mocks() {
        let mut frigate_api_mock = make_frigate_client_mock();

        // Prepare the API mock
        frigate_api_mock
            .expect_recording_clip()
            .returning(|_, _, _| Ok(Some(b"Hello world!".to_vec())))
            .once();

        // Prepare the file sender mock
        let mut file_store_mock = make_store_mock();
        file_store_mock.expect_init().returning(|| Ok(())).once();
        file_store_mock
            .expect_mkdir_p()
            .returning(|_| Ok(()))
            .once();
        file_store_mock
            .expect_put_from_memory()
            .returning(|_, _| Ok(()))
            .once();

        // Start the testing
        let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);
        let file_store_mock: Arc<dyn StoreDestination<Error = anyhow::Error>> =
            Arc::new(file_store_mock);

        let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));
        let file_sender_maker =
            Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_store_mock.clone()));

        let frigate_config = FrigateApiConfig {
            frigate_api_base_url: "http://someurl.com:5000/".to_string(),
            frigate_api_proxy: None,
        };

        let path_descriptors = PathDescriptors {
            path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
                "/home/data/".to_string().into(),
            ))]),
        };

        let review = TestReviewData {
            camera_name: "MyCamera".to_string(),
            start_time: 950.,
            end_time: 1000.,
            id: "id-abcdefg".to_string(),
            type_field: payload::TypeField::New,
        };

        let mut review_upload = ReviewUpload::new(
            Arc::new(review),
            false,
            Arc::new(frigate_config),
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,
        );

        review_upload.run().await.unwrap();
    }

    #[tokio::test]
    async fn basic_upload_in_virtual_filesystem() {
        let mut frigate_api_mock = make_frigate_client_mock();

        // Prepare the API mock
        frigate_api_mock
            .expect_recording_clip()
            .returning(|_, _, _| Ok(Some(b"Hello world!".to_vec())));

        // Prepare the file sender mock
        let file_sender = make_inmemory_filesystem();
        let file_sender_inner = file_sender.clone();

        // Start testing
        assert!(file_sender.ls(Path::new(".")).await.unwrap().is_empty());

        let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);

        let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));
        let file_sender_maker =
            Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_sender_inner.clone()));

        let frigate_config = FrigateApiConfig {
            frigate_api_base_url: "http://someurl.com:5000/".to_string(),
            frigate_api_proxy: None,
        };

        let path_descriptors = PathDescriptors {
            path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
                "/home/data/".to_string().into(),
            ))]),
        };

        let review = TestReviewData {
            camera_name: "MyCamera".to_string(),
            start_time: 950.,
            end_time: 1000.,
            id: "id-abcdefg".to_string(),
            type_field: payload::TypeField::New,
        };

        let mut review_upload = ReviewUpload::new(
            Arc::new(review.clone()),
            false,
            Arc::new(frigate_config),
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,
        );

        review_upload.run().await.unwrap();

        let dirs = file_sender.ls(Path::new(".")).await.unwrap();
        assert_eq!(dirs.len(), 1);
        assert!(file_sender.dir_exists(&dirs[0]).await.unwrap());

        let uploaded_files = file_sender
            .ls(&Path::new(".").join(&dirs[0]))
            .await
            .unwrap();

        assert_eq!(uploaded_files.len(), 1);
        assert!(
            uploaded_files[0]
                .to_str()
                .unwrap()
                .contains("RecordingClip")
        );
        assert!(uploaded_files[0].to_str().unwrap().ends_with("mp4"));
        assert!(
            uploaded_files[0]
                .to_str()
                .unwrap()
                .contains(&review.camera_name)
        );
    }
}
