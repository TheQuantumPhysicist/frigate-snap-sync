mod task;

use super::traits::{FileSenderMaker, FrigateApiMaker};
use crate::config::PathDescriptors;
use frigate_api_caller::config::FrigateApiConfig;
use futures::{StreamExt, stream::FuturesUnordered};
use mqtt_handler::types::reviews::ReviewProps;
use std::{collections::HashMap, fmt::Display, sync::Arc};
use task::SingleRecordingUploadTask;
use tokio::{sync::oneshot, task::JoinHandle};
use utils::{struct_name, time_getter::TimeGetter};

const STRUCT_NAME: &str = struct_name!(SyncSystem);

type TaskMap = HashMap<
    String,
    tokio::sync::mpsc::UnboundedSender<(Arc<dyn ReviewProps>, Option<oneshot::Sender<()>>)>,
>;

/// All recordings uploads are handled in this struct.
#[must_use]
pub struct RecordingsTaskHandler<F, S> {
    /// Commands that control this struct
    command_receiver: tokio::sync::mpsc::UnboundedReceiver<RecordingsUploadTaskHandlerCommand>,
    /// All the upload tasks futures running are here and are to be eventually joined
    running_tasks: FuturesUnordered<JoinHandle<String>>,
    /// Tasks that are running have review ids that are stored here, with a sender
    /// that can send them update objects from Frigate, coming from mqtt
    tasks_communicators: TaskMap,

    frigate_api_config: Arc<FrigateApiConfig>,
    frigate_api_maker: Arc<F>,
    file_sender_maker: Arc<S>,
    path_descriptors: PathDescriptors,

    max_retry_attempts_on_task: Option<u32>,
    retry_attempt_period: Option<std::time::Duration>,

    /// Stops the event loop
    stopped: bool,
}

pub enum RecordingsUploadTaskHandlerCommand {
    /// Send a new Review to process its recording
    Task(Arc<dyn ReviewProps>, Option<oneshot::Sender<()>>),
    /// Get the number of outstanding upload tasks running
    #[allow(dead_code)]
    GetTaskCount(oneshot::Sender<usize>),
    /// Stops the task handler by shutting down the event loop
    Stop,
}

impl<F, S> RecordingsTaskHandler<F, S>
where
    F: FrigateApiMaker,
    S: FileSenderMaker,
{
    pub fn new(
        command_receiver: tokio::sync::mpsc::UnboundedReceiver<RecordingsUploadTaskHandlerCommand>,
        frigate_api_config: Arc<FrigateApiConfig>,
        frigate_api_maker: Arc<F>,
        file_sender_maker: Arc<S>,
        path_descriptors: PathDescriptors,
        max_retry_attempts_on_task: Option<u32>,
        retry_attempt_period: Option<std::time::Duration>,
    ) -> Self {
        Self {
            running_tasks: FuturesUnordered::default(),
            command_receiver,
            tasks_communicators: HashMap::default(),
            frigate_api_config,
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,

            max_retry_attempts_on_task,
            retry_attempt_period,

            stopped: false,
        }
    }

    pub async fn run(mut self) {
        while !self.stopped {
            tokio::select! {
                Some(update) = self.command_receiver.recv() => {
                    match update {
                        RecordingsUploadTaskHandlerCommand::Stop => {
                            self.stopped = true;
                            if self.running_tasks.is_empty() {
                                break;
                            }
                        }
                        RecordingsUploadTaskHandlerCommand::Task(review, confirm_sender) => {
                            self.register_review_update(review).await;
                            if let Some(sender) = confirm_sender {
                                if sender.send(()).is_err() {
                                    tracing::error!("CRITICAL: Oneshot confirmation sender for a task in {STRUCT_NAME} failed to send. This indicates a race condition.");
                                }
                            }
                        }
                        RecordingsUploadTaskHandlerCommand::GetTaskCount(result_sender) => {
                            if result_sender.send(self.running_tasks.len()).is_err() {
                                tracing::error!("CRITICAL: Oneshot get tasks size sender for a task in {STRUCT_NAME} failed to send. This indicates a race condition.");
                            }
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

    async fn register_review_update(&mut self, review: Arc<dyn ReviewProps>) {
        let id = review.id().to_string();

        if !self.tasks_communicators.contains_key(review.id()) {
            let updates_sender = self.launch_upload_task(review.clone()).await;
            self.tasks_communicators.insert(id, updates_sender);
        }

        let sender = self
            .tasks_communicators
            .get(review.id())
            .expect("It was just inserted");

        sender
            .send((review, None))
            .expect("Invariant broken. Task communicators map could not send.");
    }

    async fn launch_upload_task(
        &self,
        review: Arc<dyn ReviewProps>,
    ) -> tokio::sync::mpsc::UnboundedSender<(Arc<dyn ReviewProps>, Option<oneshot::Sender<()>>)>
    {
        let (reviews_sender, reviews_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (first_resolve_sender, first_resolve_receiver) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::task::spawn(
            SingleRecordingUploadTask::new(
                review,
                Some(first_resolve_sender),
                reviews_receiver,
                None,
                self.frigate_api_config.clone(),
                self.frigate_api_maker.clone(),
                self.file_sender_maker.clone(),
                self.path_descriptors.clone(),
                self.max_retry_attempts_on_task,
                self.retry_attempt_period,
                TimeGetter::default(),
            )
            .start(),
        );

        first_resolve_receiver
            .await
            .expect("The task cannot die so early");

        self.running_tasks.push(handle);

        reviews_sender
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

#[cfg(test)]
mod tests;
