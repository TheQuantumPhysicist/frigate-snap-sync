#![allow(dead_code)] // TODO: remove this

mod file_upload;

use crate::{
    config::PathDescriptors,
    system::traits::{FileSenderMaker, FrigateApiMaker},
};
use frigate_api_caller::config::FrigateApiConfig;
use mqtt_handler::types::reviews::Reviews;
use std::sync::Arc;
use tokio::task::JoinHandle;

#[must_use]
pub struct SingleRecordingUploadTask<F, S> {
    // All messages received about this particular review
    reviews: Vec<Reviews>,
    // Updates about the this review is received through this channel
    receiver: tokio::sync::mpsc::UnboundedReceiver<Box<Reviews>>,

    frigate_api_config: Arc<FrigateApiConfig>,
    frigate_api_maker: Arc<F>,
    file_sender_maker: Arc<S>,
    path_descriptors: PathDescriptors,
}

impl<F, S> SingleRecordingUploadTask<F, S>
where
    F: FrigateApiMaker,
    S: FileSenderMaker,
{
    pub fn new(
        receiver: tokio::sync::mpsc::UnboundedReceiver<Box<Reviews>>,
        frigate_api_config: Arc<FrigateApiConfig>,
        frigate_api_maker: Arc<F>,
        file_sender_maker: Arc<S>,
        path_descriptors: PathDescriptors,
    ) -> Self {
        Self {
            reviews: Vec::default(),
            receiver,
            frigate_api_config,
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,
        }
    }

    pub async fn start(mut self) -> JoinHandle<String> {
        loop {
            tokio::select! {
                Some(review) = self.receiver.recv() => {
                    self.on_received_review(review).await;
                }
            }
        }
    }

    #[allow(clippy::unused_async)] // TODO: remove this
    pub async fn on_received_review(&mut self, review: Box<Reviews>) {
        self.reviews.push(review.as_ref().clone());

        // let alternative_upload = false; // TODO: figure out the algo for this

        // ReviewUpload::new(
        //     *review,
        //     alternative_upload,
        //     self.frigate_api_config.clone(),
        //     self.frigate_api_maker.clone(),
        //     self.file_sender_maker.clone(),
        //     self.path_descriptors.clone(),
        // )
        // .run();
    }
}
