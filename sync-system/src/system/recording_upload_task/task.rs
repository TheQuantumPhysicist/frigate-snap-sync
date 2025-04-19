#![allow(dead_code)] // TODO: remove this

use std::sync::Arc;

use frigate_api_caller::config::FrigateApiConfig;
use mqtt_handler::types::reviews::Reviews;
use tokio::task::JoinHandle;

use crate::system::traits::{FileSenderMaker, FrigateApiMaker};

#[must_use]
pub struct RecordingUploadTask<F, S> {
    reviews: Vec<Reviews>,
    receiver: tokio::sync::mpsc::UnboundedReceiver<Box<Reviews>>,
    frigate_api_config: Arc<FrigateApiConfig>,
    frigate_api_maker: Arc<F>,
    file_sender_maker: Arc<S>,
}

impl<F, S> RecordingUploadTask<F, S>
where
    F: FrigateApiMaker,
    S: FileSenderMaker,
{
    pub fn new(
        receiver: tokio::sync::mpsc::UnboundedReceiver<Box<Reviews>>,
        frigate_api_config: Arc<FrigateApiConfig>,
        frigate_api_maker: Arc<F>,
        file_sender_maker: Arc<S>,
    ) -> Self {
        Self {
            reviews: Vec::default(),
            receiver,
            frigate_api_config,
            frigate_api_maker,
            file_sender_maker,
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
        self.reviews.push(*review);

        // TODO: continue the implementation:
        // 1. Get the current video
        // 2. Upload it
        // 3. If this is the "end", shut down the task
        // 4. After a long enough timeout without any message received, kill the task anyway
    }
}
