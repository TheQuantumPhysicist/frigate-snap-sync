use file_sender::{make_store, path_descriptor::PathDescriptor};
use frigate_api_caller::{config::FrigateApiConfig, traits::FrigateApi};
use mocks::frigate_api::make_frigate_client_mock;
use mqtt_handler::types::{CapturedPayloads, snapshot::Snapshot};
use rstest::rstest;
use std::sync::Arc;
use test_utils::random::{
    Seed, gen_random_bytes, gen_random_string, make_seedable_rng, random_seed,
};
use tokio::sync::{mpsc::UnboundedSender, oneshot};

use crate::{config::PathDescriptors, state::CamerasState, system::SyncSystem};

async fn get_camera_state(sender: &UnboundedSender<oneshot::Sender<CamerasState>>) -> CamerasState {
    let (state_sender, state_receiver) = oneshot::channel();
    sender.send(state_sender).unwrap();
    state_receiver.await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[rstest]
async fn basic(random_seed: Seed, #[values(false, true)] pass_initial_api_test: bool) {
    let mut rng = make_seedable_rng(random_seed);

    let temp_dir = tempfile::TempDir::new().unwrap();
    let upload_dests = Arc::new(vec![Arc::new(PathDescriptor::Local(
        temp_dir.path().to_owned(),
    ))]);
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
        upload_dests,
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

        // TODO: assert that filesystem(s) don't have anything uploaded here
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
