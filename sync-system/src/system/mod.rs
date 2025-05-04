mod common;
mod recording_upload_handler;
mod snapshot_upload_task;
pub mod traits;

use crate::{
    config::{PathDescriptors, VideoSyncConfig},
    state::CamerasState,
};
use file_sender::{path_descriptor::PathDescriptor, traits::StoreDestination};
use frigate_api_caller::{config::FrigateApiConfig, traits::FrigateApi};
use futures::FutureExt;
use mqtt_handler::types::{CapturedPayloads, reviews::ReviewProps, snapshot::Snapshot};
use recording_upload_handler::{RecordingsTaskHandler, RecordingsUploadTaskHandlerCommand};
use snapshot_upload_task::{SnapshotsTaskHandler, SnapshotsUploadTaskHandlerCommand};
use std::{path::Path, sync::Arc};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};
use traits::{FileSenderMaker, FrigateApiMaker};
use utils::struct_name;

const STRUCT_NAME: &str = struct_name!(SyncSystem);
const SLEEP_TIME_ON_API_ERROR: std::time::Duration = std::time::Duration::from_secs(10);

pub struct SyncSystem<F, S> {
    cameras_state: CamerasState,
    config: VideoSyncConfig,

    frigate_api_config: Arc<FrigateApiConfig>,
    frigate_api_maker: Arc<F>,
    file_sender_maker: Arc<S>,

    rec_updates_sender: UnboundedSender<RecordingsUploadTaskHandlerCommand>,
    snapshots_updates_sender: UnboundedSender<SnapshotsUploadTaskHandlerCommand>,
    mqtt_data_receiver: tokio::sync::mpsc::UnboundedReceiver<CapturedPayloads>,

    join_handles: Vec<(String, JoinHandle<()>)>,

    stop_receiver: Option<UnboundedReceiver<()>>,
}

impl<F, S> SyncSystem<F, S>
where
    F: FrigateApiMaker,
    S: FileSenderMaker,
{
    pub fn new(
        config: VideoSyncConfig,
        frigate_api_maker: F,
        file_sender_maker: S,
        mqtt_data_receiver: tokio::sync::mpsc::UnboundedReceiver<CapturedPayloads>,
        stop_receiver: Option<UnboundedReceiver<()>>,
    ) -> Self {
        let frigate_api_config = FrigateApiConfig::from(&config);

        let frigate_api_config = frigate_api_config.clone();
        let path_descriptors = config.upload_destinations().clone();

        let frigate_api_maker = Arc::new(frigate_api_maker);
        let frigate_api_config = Arc::new(frigate_api_config);
        let file_sender_maker = Arc::new(file_sender_maker);

        let (rec_updates_sender, rec_updates_receiver) = tokio::sync::mpsc::unbounded_channel();
        let rec_handler_task = Self::run_reviews_task_handler(
            rec_updates_receiver,
            frigate_api_maker.clone(),
            frigate_api_config.clone(),
            file_sender_maker.clone(),
            path_descriptors.clone(),
        );

        let (snapshots_updates_sender, snapshots_updates_receiver) =
            tokio::sync::mpsc::unbounded_channel();
        let snapshots_task_join_handler = Self::run_snapshots_task_handler(
            snapshots_updates_receiver,
            file_sender_maker.clone(),
            path_descriptors,
        );

        let join_handles = vec![
            ("recordings handler".to_string(), rec_handler_task),
            ("snapshots handler".to_string(), snapshots_task_join_handler),
        ];

        Self {
            cameras_state: CamerasState::default(),
            config,

            frigate_api_config,
            frigate_api_maker,
            file_sender_maker,

            rec_updates_sender,
            snapshots_updates_sender,
            mqtt_data_receiver,

            join_handles,

            stop_receiver,
        }
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
        self.test_frigate_api_connection().await;

        self.test_file_senders().await;

        loop {
            let stop_receiver = match self.stop_receiver.as_mut() {
                Some(receiver) => receiver.recv().boxed(),
                None => futures::future::pending().boxed(),
            };

            tokio::select! {
                Some(data) = self.mqtt_data_receiver.recv() => {
                    self.on_mqtt_data_received(data);
                },

                Some(()) = stop_receiver => {
                    tracing::info!("Received stop signal to stop {STRUCT_NAME}.");
                    break;
                }
            }
        }

        tracing::info!("Reached the end of {STRUCT_NAME} event loop. Unwinding all task managers.");

        self.rec_updates_sender
            .send(RecordingsUploadTaskHandlerCommand::Stop)
            .expect("Sending stop signal for recordings handler failed");

        self.snapshots_updates_sender
            .send(SnapshotsUploadTaskHandlerCommand::Stop)
            .expect("Sending stop signal for snapshots handler failed");

        for (task_name, join_handle) in &mut self.join_handles {
            match join_handle.await {
                Ok(()) => tracing::info!("Joining {task_name} task completed successfully"),
                Err(e) => tracing::error!("CRITICAL: Failed to join {task_name} task: {e}"),
            }
        }

        tracing::info!("Unwinding of {STRUCT_NAME} done.");

        Ok(())
    }

    fn on_mqtt_data_received(&mut self, data: CapturedPayloads) {
        match data {
            CapturedPayloads::CameraRecordingsState(recordings_state) => {
                tracing::info!(
                    "{STRUCT_NAME}: Updating recordings state of camera `{}` to `{}`",
                    recordings_state.camera_label,
                    recordings_state.state
                );

                self.cameras_state
                    .update_recordings_state(recordings_state.camera_label, recordings_state.state);
            }
            CapturedPayloads::CameraSnapshotsState(snapshots_state) => {
                tracing::info!(
                    "{STRUCT_NAME}: Updating snapshots state of camera `{}` to `{}`",
                    snapshots_state.camera_label,
                    snapshots_state.state
                );

                self.cameras_state
                    .update_snapshots_state(snapshots_state.camera_label, snapshots_state.state);
            }
            CapturedPayloads::Snapshot(snapshot) => {
                tracing::info!(
                    "{STRUCT_NAME}: Received snapshot from camera: `{}`. Size: `{}`",
                    snapshot.camera_label,
                    snapshot.image_bytes.len()
                );

                self.handle_snapshot_payload(snapshot);
            }
            CapturedPayloads::Reviews(review) => {
                tracing::info!(
                    "{STRUCT_NAME}: Received review from camera: {}, with id: {}",
                    review.camera_name(),
                    review.id()
                );

                self.handle_review_payload(review);
            }
        }
    }

    pub fn make_frigate_api(&self) -> anyhow::Result<Arc<dyn FrigateApi>> {
        (self.frigate_api_maker)(&self.frigate_api_config)
    }

    #[allow(clippy::type_complexity)]
    #[must_use]
    pub fn make_file_senders(
        &self,
    ) -> Vec<(
        Arc<PathDescriptor>,
        anyhow::Result<Arc<dyn StoreDestination<Error = anyhow::Error>>>,
    )> {
        self.config
            .upload_destinations()
            .path_descriptors
            .iter()
            .map(|d| (d.clone(), (self.file_sender_maker)(d)))
            .collect()
    }

    pub async fn test_frigate_api_connection(&self) {
        let api = self
            .make_frigate_api()
            .expect("Creating Frigate API failed");
        match api.as_ref().test_call().await {
            Ok(()) => {
                tracing::info!("Initial test connection to Frigate API succeeded.");
            }
            Err(e) => {
                tracing::error!(
                    "Error: failed to make test connection to the Frigate API. This could mean that the API is temporarily down, or that the address you used is wrong. The software will keep attempting to connect when needed. Error: {e}"
                );

                tokio::time::sleep(SLEEP_TIME_ON_API_ERROR).await;
            }
        }
    }

    pub async fn test_file_senders(&self) {
        let senders = self.make_file_senders();
        for (descriptor, sender_result) in senders {
            match sender_result {
                Ok(s) => match s.as_ref().ls(Path::new(".")).await {
                    Ok(_) => {
                        tracing::info!("Basic file sender test for `{descriptor}` succeeded!");
                    }
                    Err(e) => {
                        tracing::error!(
                            "Basic file sender test failed for descriptor `{descriptor}`: {e}",
                        );
                    }
                },
                Err(e) => {
                    tracing::error!(
                        "Failed to create file sender with descriptor `{descriptor}`: {e}",
                    );
                }
            }
        }
    }

    fn handle_snapshot_payload(&mut self, snapshot: Arc<Snapshot>) {
        if self
            .cameras_state
            .camera_snapshots_state(&snapshot.camera_label)
        {
            let camera_name = snapshot.camera_label.clone();
            tracing::debug!("Sending snapshot for camera {camera_name}");

            let send_res = self
                .snapshots_updates_sender
                .send(SnapshotsUploadTaskHandlerCommand::Task(snapshot, None));

            match send_res {
                Ok(()) => {
                    tracing::trace!(
                        "Sent new task snapshot upload task successfully for camera {camera_name}"
                    );
                }
                Err(e) => tracing::error!(
                    "CRITICAL: Failed to send message to snapshots upload handler: {e}"
                ),
            }
        } else {
            tracing::debug!(
                "Ignoring snapshot from camera: {} - Snapshots are disabled in Frigate.",
                snapshot.camera_label
            );
        }
    }

    fn handle_review_payload(&mut self, review: Arc<dyn ReviewProps>) {
        if self
            .cameras_state
            .camera_recordings_state(review.camera_name())
        {
            let camera_name = review.camera_name().to_string();
            let id = review.id().to_string();
            tracing::debug!("Sending review for camera {camera_name} with id {id}");

            let send_res = self
                .rec_updates_sender
                .send(RecordingsUploadTaskHandlerCommand::Task(review, None));

            match send_res {
                Ok(()) => tracing::trace!(
                    "Sent new recording upload task successfully for camera {camera_name} with id {id}"
                ),
                Err(e) => tracing::error!(
                    "CRITICAL: Failed to send message to recordings upload handler: {e}"
                ),
            }
        } else {
            tracing::debug!(
                "Ignoring review from camera: `{}` - Recordings are disabled in Frigate.",
                review.camera_name()
            );
        }
    }

    fn run_reviews_task_handler(
        rec_updates_receiver: UnboundedReceiver<RecordingsUploadTaskHandlerCommand>,
        frigate_api_maker: Arc<F>,
        frigate_api_config: Arc<FrigateApiConfig>,
        file_sender_maker: Arc<S>,
        path_descriptors: PathDescriptors,
    ) -> JoinHandle<()> {
        tokio::task::spawn(async move {
            RecordingsTaskHandler::new(
                rec_updates_receiver,
                frigate_api_config,
                frigate_api_maker,
                file_sender_maker,
                path_descriptors,
                None,
                None,
            )
            .run()
            .await;
        })
    }

    fn run_snapshots_task_handler(
        command_receiver: UnboundedReceiver<SnapshotsUploadTaskHandlerCommand>,
        file_sender_maker: Arc<S>,
        path_descriptors: PathDescriptors,
    ) -> JoinHandle<()> {
        tokio::task::spawn(
            SnapshotsTaskHandler::new(command_receiver, file_sender_maker, path_descriptors).run(),
        )
    }
}
