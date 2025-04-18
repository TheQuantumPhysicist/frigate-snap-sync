mod common;
mod snapshot_task;
pub mod traits;

use crate::{config::VideoSyncConfig, state::CamerasState};
use file_sender::{path_descriptor::PathDescriptor, traits::StoreDestination};
use frigate_api_caller::{config::FrigateApiConfig, traits::FrigateApi};
use futures::{StreamExt, stream::FuturesUnordered};
use mqtt_handler::{
    config::MqttHandlerConfig,
    types::{CapturedPayloads, snapshot::Snapshot},
};
use snapshot_task::SnapshotTask;
use std::{path::Path, sync::Arc};
use tokio::task::JoinHandle;
use traits::{FileSenderMaker, FrigateApiMaker};

const MAX_ATTEMPT_COUNT: u32 = 128;

macro_rules! struct_name {
    ($t:ty) => {
        stringify!($t)
    };
}

const STRUCT_NAME: &str = struct_name!(SyncSystem);

const SLEEP_TIME_ON_API_ERROR: std::time::Duration = std::time::Duration::from_secs(10);

pub struct SyncSystem<F, S> {
    cameras_state: CamerasState,
    config: VideoSyncConfig,
    frigate_api_config: Arc<FrigateApiConfig>,
    frigate_api_maker: Arc<F>,
    file_sender_maker: Arc<S>,
    tasks_handles: FuturesUnordered<JoinHandle<()>>,
}

impl<F, S> SyncSystem<F, S>
where
    F: FrigateApiMaker,
    S: FileSenderMaker,
{
    pub fn new(config: VideoSyncConfig, frigate_api_maker: F, file_sender_maker: S) -> Self {
        let frigate_api_config = FrigateApiConfig::from(&config);
        Self {
            cameras_state: CamerasState::default(),
            config,
            frigate_api_config: Arc::new(frigate_api_config),
            frigate_api_maker: Arc::new(frigate_api_maker),
            tasks_handles: FuturesUnordered::default(),
            file_sender_maker: Arc::new(file_sender_maker),
        }
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let mqtt_config = MqttHandlerConfig::from(&self.config);

        let (data_sender, mut data_receiver) = tokio::sync::mpsc::unbounded_channel();

        let mut handler = mqtt_handler::MqttHandler::new(mqtt_config, data_sender)?;

        self.test_frigate_api_connection().await;

        tokio::time::sleep(SLEEP_TIME_ON_API_ERROR).await;

        self.test_file_senders();

        tokio::select! {
            Some(data) = data_receiver.recv() => {
                match data {
                    CapturedPayloads::CameraRecordingsState(recordings_state) => {
                        tracing::info!(
                            "{STRUCT_NAME}: Updating recordings state of camera {} to {}",
                            recordings_state.camera_label,
                            recordings_state.state
                        );

                        self.cameras_state.update_recordings_state(
                            recordings_state.camera_label,
                            recordings_state.state,
                        );
                    }
                    CapturedPayloads::CameraSnapshotsState(snapshots_state) => {
                        tracing::info!(
                            "{STRUCT_NAME}: Updating snapshots state of camera {} to {}",
                            snapshots_state.camera_label,
                            snapshots_state.state
                        );

                        self.cameras_state.update_snapshots_state(
                            snapshots_state.camera_label,
                            snapshots_state.state,
                        );
                    }
                    CapturedPayloads::Snapshot(snapshot) => {
                        tracing::info!(
                            "{STRUCT_NAME}: Received snapshot from camera: {}. Size: {}",
                            snapshot.camera_label,
                            snapshot.image.as_bytes().len()
                        );

                        if self
                            .cameras_state
                            .camera_snapshots_state(&snapshot.camera_label)
                        {
                            self.launch_snapshot_upload_task(snapshot);
                        } else {
                            tracing::debug!(
                                "Ignoring snapshot from camera: {} - Snapshots are disabled in Frigate.",
                                snapshot.camera_label
                            );
                        }
                    }
                    CapturedPayloads::Reviews(review) => {
                        tracing::info!(
                            "{STRUCT_NAME}: Received review from camera: {}, with id: {}",
                            review.camera_name(),
                            review.id()
                        );

                        if self
                            .cameras_state
                            .camera_recordings_state(review.camera_name())
                        {
                            // TODO: launch task to do the uploads of review

                            // let api = match self.make_frigate_api() {
                            //     Ok(a) => a,
                            //     Err(e) => tracing::error!("Failed to connect"),
                            // };
                        } else {
                            tracing::debug!(
                                "Ignoring review from camera: {} - Recordings are disabled in Frigate.",
                                review.camera_name()
                            );
                        }
                    }
                }
            },

            Some(task_result) = self.tasks_handles.next() => {
                if let Err(e) = task_result {
                    tracing::error!("Task exited with error: {e}");
                }
            }
        }

        handler.wait().await;

        Ok(())
    }

    pub fn make_frigate_api(&self) -> anyhow::Result<Box<dyn FrigateApi>> {
        (self.frigate_api_maker)(&self.frigate_api_config)
    }

    #[allow(clippy::type_complexity)]
    pub fn make_file_senders(
        &self,
    ) -> Vec<(
        Arc<PathDescriptor>,
        anyhow::Result<Box<dyn StoreDestination<Error = anyhow::Error>>>,
    )> {
        self.config
            .upload_destinations()
            .path_descriptors
            .iter()
            .map(|d| (d.clone(), (self.file_sender_maker)(d)))
            .collect::<Vec<_>>()
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
            }
        }
    }

    pub fn test_file_senders(&self) {
        let senders = self.make_file_senders();
        for (descriptor, sender_result) in senders {
            match sender_result {
                Ok(s) => match s.as_ref().ls(Path::new(".")) {
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

    pub fn launch_snapshot_upload_task(&self, snapshot: Snapshot) {
        let path_descriptors = self.config.upload_destinations().clone();
        let file_sender_maker = self.file_sender_maker.clone();
        let handle = tokio::task::spawn(async move {
            let snapshot = snapshot;
            let task = SnapshotTask::new(snapshot, file_sender_maker, path_descriptors);
            task.launch();
        });
        self.tasks_handles.push(handle);
    }

    // pub async fn launch_recording_upload_task(&self, review: Box<Review>) {
    //     let api_maker = self.frigate_api_maker.clone();
    //     let api_config = self.frigate_api_config.clone();
    //     let handle = tokio::task::spawn(async move {
    //         let api_maker = api_maker;
    //         let api_config = api_config;
    //         let review = review;
    //     });
    // }
}
