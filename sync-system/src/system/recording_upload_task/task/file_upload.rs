use crate::{
    config::PathDescriptors,
    system::traits::{FileSenderMaker, FrigateApiMaker},
};
use frigate_api_caller::config::FrigateApiConfig;
use mqtt_handler::types::reviews::Reviews;
use std::sync::Arc;

pub struct ReviewUpload<F, S> {
    review: Reviews,
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
    path_descriptors: PathDescriptors,
}

impl<F, S> ReviewUpload<F, S>
where
    F: FrigateApiMaker,
    S: FileSenderMaker,
{
    pub fn new(
        review: Reviews,
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
            path_descriptors,
        }
    }

    // pub fn run(mut self) {

    // TODO: continue the implementation:
    // 1. Get the current video
    // 2. Upload it
    // 3. If this is the "end", shut down the task
    // 4. After a long enough timeout without any message received, kill the task anyway

    //     loop {
    //         match self.state {
    //             ReviewUploadState::Start => self.state = ReviewUploadState::GettingVideoFromAPI,
    //             ReviewUploadState::GettingVideoFromAPI => {
    //                 // let mut remaining_descriptors = self
    //                 //     .file_senders_path_descriptors
    //                 //     .path_descriptors
    //                 //     .as_ref()
    //                 //     .clone();

    //                 // make_file_senders(self.file_sender_maker.clone(), remaining_path_descriptors);
    //             }
    //             ReviewUploadState::UploadToStore => {}
    //         }
    //     }
    // }
}

#[derive(Debug, Clone, Default)]
pub enum ReviewUploadState {
    #[default]
    Start,
    GettingVideoFromAPI,
    UploadToStore,
}
