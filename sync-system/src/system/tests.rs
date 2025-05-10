use crate::{config::PathDescriptors, state::CamerasState, system::SyncSystem};
use file_sender::{make_store, path_descriptor::PathDescriptor};
use frigate_api_caller::{config::FrigateApiConfig, traits::FrigateApi};
use mocks::frigate_api::make_frigate_client_mock;
use mqtt_handler::types::{
    CapturedPayloads,
    reviews::{ReviewProps, payload},
    snapshot::Snapshot,
    snapshots_state::SnapshotsState,
};
use rstest::rstest;
use std::{path::Path, sync::Arc};
use test_utils::{
    asserts::{assert_str_contains, assert_str_starts_with},
    random::{Seed, gen_random_bytes, gen_random_string, make_seedable_rng, random_seed},
};
use tokio::sync::{mpsc::UnboundedSender, oneshot};

async fn get_camera_state(sender: &UnboundedSender<oneshot::Sender<CamerasState>>) -> CamerasState {
    let (state_sender, state_receiver) = oneshot::channel();
    sender.send(state_sender).unwrap();
    state_receiver.await.unwrap()
}

#[derive(Debug, Clone)]
struct TestReviewData {
    camera_name: String,
    start_time: f64,
    end_time: Option<f64>,
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
        self.end_time
    }

    fn type_field(&self) -> payload::TypeField {
        self.type_field
    }
}

#[tokio::test]
#[rstest]
async fn basic(random_seed: Seed, #[values(false, true)] pass_initial_api_test: bool) {
    let mut rng = make_seedable_rng(random_seed);

    let temp_dir1 = tempfile::TempDir::new().unwrap();
    let temp_dir2 = tempfile::TempDir::new().unwrap();
    let upload_dests = Arc::new(vec![
        Arc::new(PathDescriptor::Local(temp_dir1.path().to_owned())),
        Arc::new(PathDescriptor::Local(temp_dir2.path().to_owned())),
    ]);
    let upload_dests = PathDescriptors {
        path_descriptors: upload_dests,
    };

    let frigate_api_config = FrigateApiConfig {
        frigate_api_base_url: "http://example.com".to_string(),
        frigate_api_proxy: None,
    };

    let mut frigate_api_mock = make_frigate_client_mock();
    {
        frigate_api_mock.expect_test_call().returning(move || {
            if pass_initial_api_test {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Fake api error for tests"))
            }
        });
    }
    let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);
    let frigate_api_maker = move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone());

    let file_sender_maker = move |pd: &Arc<PathDescriptor>| make_store(pd);

    let (mqtt_data_sender, mqtt_data_receiver) =
        tokio::sync::mpsc::unbounded_channel::<CapturedPayloads>();

    let (stop_sender, stop_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (camera_state_getter_sender, camera_state_getter_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let sync_sys = SyncSystem::new(
        upload_dests.clone(),
        Arc::new(frigate_api_config),
        frigate_api_maker,
        file_sender_maker,
        mqtt_data_receiver,
        Some(camera_state_getter_receiver),
        Some(stop_receiver),
    );

    let task_handle = tokio::task::spawn(async move { sync_sys.start().await });

    {
        let camera_state = get_camera_state(&camera_state_getter_sender).await;
        assert!(camera_state.recordings_state().is_empty());
        assert!(camera_state.snapshots_state().is_empty());
    }

    {
        let snapshot = Snapshot {
            image_bytes: gen_random_bytes(&mut rng, 100..1000),
            camera_label: gen_random_string(&mut rng, 10..20),
            object_name: gen_random_string(&mut rng, 10..20),
        };
        let payload = CapturedPayloads::Snapshot(Arc::new(snapshot));
        mqtt_data_sender.send(payload).unwrap();

        for pd in &*upload_dests.path_descriptors {
            let file_sender = file_sender_maker(pd).unwrap();
            // No upload because the state of snapshots is disabled by default
            assert!(file_sender.ls(Path::new(".")).await.unwrap().is_empty());
        }
    }

    {
        let review = TestReviewData {
            camera_name: gen_random_string(&mut rng, 10..20),
            start_time: 950.,
            end_time: None,
            id: "id-abcdefg".to_string(),
            type_field: payload::TypeField::New,
        };
        let payload = CapturedPayloads::Reviews(Arc::new(review));
        mqtt_data_sender.send(payload).unwrap();

        for pd in &*upload_dests.path_descriptors {
            let file_sender = file_sender_maker(pd).unwrap();
            // No upload because the state of reviews is disabled by default
            assert!(file_sender.ls(Path::new(".")).await.unwrap().is_empty());
        }
    }

    let camera1_label = "camera1_label";

    {
        {
            let camera_state = get_camera_state(&camera_state_getter_sender).await;
            assert!(camera_state.recordings_state().is_empty());
            assert!(camera_state.snapshots_state().is_empty());
        }

        {
            let enable_payload = CapturedPayloads::CameraSnapshotsState(SnapshotsState {
                camera_label: camera1_label.to_string(),
                state: true,
            });
            mqtt_data_sender.send(enable_payload).unwrap();
        }

        {
            {
                // We can't guarantee that the mqtt state update will happen in order, so we just wait for it for a while
                tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    loop {
                        if !get_camera_state(&camera_state_getter_sender)
                            .await
                            .snapshots_state()
                            .is_empty()
                        {
                            break;
                        }
                    }
                    futures::future::ready(()).await;
                })
                .await
                .unwrap();
            }
            let camera_state = get_camera_state(&camera_state_getter_sender).await;
            assert_eq!(camera_state.snapshots_state().len(), 1);
            assert_eq!(camera_state.recordings_state().len(), 0);
        }
    }

    {
        let snapshot = Snapshot {
            image_bytes: gen_random_bytes(&mut rng, 100..1000),
            camera_label: camera1_label.to_string(),
            object_name: gen_random_string(&mut rng, 10..20),
        };
        let payload = CapturedPayloads::Snapshot(Arc::new(snapshot));
        mqtt_data_sender.send(payload).unwrap();

        for pd in &*upload_dests.path_descriptors {
            let file_sender = file_sender_maker(pd).unwrap();

            {
                // We can't guarantee that the upload will happen before we check, so we gotta wait for it
                tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    loop {
                        let dirs = file_sender.ls(Path::new(".")).await.unwrap();
                        if !dirs.is_empty() && !file_sender.ls(&dirs[0]).await.unwrap().is_empty() {
                            break;
                        }
                    }
                    futures::future::ready(()).await;
                })
                .await
                .unwrap();
            }

            // No upload because the state of snapshots is disabled by default
            let dirs = file_sender.ls(Path::new(".")).await.unwrap();
            assert_eq!(dirs.len(), 1);
            // Expect one file
            let files = file_sender.ls(&dirs[0]).await.unwrap();
            assert_eq!(files.len(), 1);
            assert_str_starts_with(&files[0].display().to_string(), "Snapshot");
            assert_str_contains(&files[0].display().to_string(), camera1_label);
        }
    }

    // TODO: test changing camera states, and responding to snapshots and recordings
    // TODO: test receiving snapshots with both camera states, on/off
    // TODO: test receiving recordings with both camera states, on/off

    // Shutdown mechanism
    {
        stop_sender.send(()).unwrap();

        task_handle.await.unwrap().unwrap();
    }
}
