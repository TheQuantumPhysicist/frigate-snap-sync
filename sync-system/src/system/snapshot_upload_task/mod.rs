mod task;

use super::traits::FileSenderMaker;
use crate::config::PathDescriptors;
use futures::{StreamExt, stream::FuturesUnordered};
use mqtt_handler::types::snapshot::Snapshot;
use std::{fmt::Display, sync::Arc};
use task::SnapshotUploadTask;
use tokio::{sync::oneshot, task::JoinHandle};
use utils::struct_name;

const STRUCT_NAME: &str = struct_name!(SyncSystem);

pub struct SnapshotsTaskHandler<S> {
    /// Commands that control this struct
    command_receiver: tokio::sync::mpsc::UnboundedReceiver<SnapshotsUploadTaskHandlerCommand>,

    file_sender_maker: Arc<S>,
    path_descriptors: PathDescriptors,

    running_tasks: FuturesUnordered<JoinHandle<()>>,

    /// Stops the event loop
    stopped: bool,
}

pub enum SnapshotsUploadTaskHandlerCommand {
    /// Send a new Review to process its snapshot
    Task(Arc<Snapshot>, Option<oneshot::Sender<()>>),
    /// Get the number of outstanding upload tasks running
    #[allow(dead_code)]
    GetTaskCount(oneshot::Sender<usize>),
    /// Stops the task handler by shutting down the event loop
    Stop,
}

impl<S> SnapshotsTaskHandler<S>
where
    S: FileSenderMaker,
{
    pub fn new(
        command_receiver: tokio::sync::mpsc::UnboundedReceiver<SnapshotsUploadTaskHandlerCommand>,
        file_sender_maker: Arc<S>,
        path_descriptors: PathDescriptors,
    ) -> Self {
        SnapshotsTaskHandler {
            command_receiver,
            file_sender_maker,
            path_descriptors,

            running_tasks: FuturesUnordered::default(),

            stopped: false,
        }
    }

    pub async fn run(mut self) {
        while !self.stopped {
            tokio::select! {
                Some(update) = self.command_receiver.recv() => {
                    match update {
                        SnapshotsUploadTaskHandlerCommand::Stop => {
                            self.stopped = true;
                            if self.running_tasks.is_empty() {
                                break;
                            }
                        }
                        SnapshotsUploadTaskHandlerCommand::Task(snapshot, confirm_sender) => {
                            self.launch_snapshot_upload_task(snapshot);
                            if let Some(sender) = confirm_sender {
                                if sender.send(()).is_err() {
                                    tracing::error!("CRITICAL: Oneshot confirmation sender for a task in {STRUCT_NAME} failed to send. This indicates a race condition.");
                                }
                            }
                        }
                        SnapshotsUploadTaskHandlerCommand::GetTaskCount(result_sender) => {
                            if result_sender.send(self.running_tasks.len()).is_err() {
                                tracing::error!("CRITICAL: Oneshot get tasks size sender for a task in {STRUCT_NAME} failed to send. This indicates a race condition.");
                            }
                        }
                    }
                }

                Some(task_result) = self.running_tasks.next() => {
                    Self::on_task_joined(task_result);

                    if self.running_tasks.is_empty() && self.stopped {
                        break;
                    }
                }
            }
        }
    }

    fn on_task_joined<E: Display>(task_result: Result<(), E>) {
        match task_result {
            Ok(()) => {
                tracing::info!("Snapshot task joined successfully");
            }
            Err(e) => tracing::error!(
                "CRITICAL. Snapshot task joined with error: {e}. This can lead to a memory leak!"
            ),
        }
    }

    fn launch_snapshot_upload_task(&self, snapshot: Arc<Snapshot>) {
        let path_descriptors = self.path_descriptors.clone();
        let file_sender_maker = self.file_sender_maker.clone();
        let handle = tokio::task::spawn(async move {
            let snapshot = snapshot;
            let task = SnapshotUploadTask::new(snapshot, file_sender_maker, path_descriptors);
            task.run().await;
        });
        self.running_tasks.push(handle);
    }
}

#[cfg(test)]
mod tests;
