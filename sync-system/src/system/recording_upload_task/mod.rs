mod task;

use crate::config::PathDescriptors;
use frigate_api_caller::config::FrigateApiConfig;
use futures::{StreamExt, stream::FuturesUnordered};
use mqtt_handler::types::reviews::Reviews;
use std::{collections::HashMap, fmt::Display, sync::Arc};
use task::SingleRecordingUploadTask;
use tokio::task::JoinHandle;

use super::traits::{FileSenderMaker, FrigateApiMaker};

pub struct RecordingTaskHandler<F, S> {
    running_tasks: FuturesUnordered<JoinHandle<String>>,
    update_receiver: tokio::sync::mpsc::UnboundedReceiver<RecordingTaskHandlerUpdate>,
    stopped: bool,
    tasks_communicators: HashMap<String, tokio::sync::mpsc::UnboundedSender<Arc<Reviews>>>,
    frigate_api_config: Arc<FrigateApiConfig>,
    frigate_api_maker: Arc<F>,
    file_sender_maker: Arc<S>,
    path_descriptors: PathDescriptors,
}

pub enum RecordingTaskHandlerUpdate {
    Stop,
    Task(Arc<Reviews>),
}

impl<F, S> RecordingTaskHandler<F, S>
where
    F: FrigateApiMaker,
    S: FileSenderMaker,
{
    pub fn new(
        update_receiver: tokio::sync::mpsc::UnboundedReceiver<RecordingTaskHandlerUpdate>,
        frigate_api_config: Arc<FrigateApiConfig>,
        frigate_api_maker: Arc<F>,
        file_sender_maker: Arc<S>,
        path_descriptors: PathDescriptors,
    ) -> Self {
        Self {
            running_tasks: FuturesUnordered::default(),
            update_receiver,
            stopped: false,
            tasks_communicators: HashMap::default(),
            frigate_api_config,
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(update) = self.update_receiver.recv() => {
                    match update {
                        RecordingTaskHandlerUpdate::Stop => {
                            self.stopped = true;
                            if self.running_tasks.is_empty() {
                                break;
                            }
                        }
                        RecordingTaskHandlerUpdate::Task(review) => {
                            self.register_review_update(review).await;
                        }
                    }
                }

                Some(task_result) = self.running_tasks.next() => {
                    self.on_task_joined(task_result);

                    if self.running_tasks.is_empty() && self.stopped {
                        break;
                    }
                }
            }
        }
    }

    async fn register_review_update(&mut self, review: Arc<Reviews>) {
        let id = &review.id();

        if !self.tasks_communicators.contains_key(review.id()) {
            let sender = self.launch_upload_task().await;
            self.tasks_communicators.insert((*id).to_string(), sender);
        }

        let sender = self
            .tasks_communicators
            .get(review.id())
            .expect("It was just inserted");

        sender
            .send(review)
            .expect("Invariant broken. Task communicators map could not send.");
    }

    async fn launch_upload_task(&self) -> tokio::sync::mpsc::UnboundedSender<Arc<Reviews>> {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let handle = SingleRecordingUploadTask::new(
            receiver,
            self.frigate_api_config.clone(),
            self.frigate_api_maker.clone(),
            self.file_sender_maker.clone(),
            self.path_descriptors.clone(),
        )
        .start()
        .await;
        self.running_tasks.push(handle);
        sender
    }

    fn on_task_joined<E: Display>(&mut self, task_result: Result<String, E>) {
        match task_result {
            Ok(id) => {
                tracing::info!("Recording task for id `{id}` joined successfully");
                self.tasks_communicators
                    .remove(&id)
                    .expect("The value must have been inserted before");
            }
            Err(e) => tracing::error!(
                "CRITICAL. Recording task joined with error: {e}. This can lead to a memory leak!"
            ),
        }
    }
}
