use file_sender::{make_store, path_descriptor::PathDescriptor};
use frigate_api_caller::{config::FrigateApiConfig, traits::FrigateApi};
use mocks::frigate_api::make_frigate_client_mock;
use mqtt_handler::types::CapturedPayloads;
use rstest::rstest;
use std::sync::Arc;
use test_utils::random::{Seed, make_seedable_rng, random_seed};

use crate::{config::PathDescriptors, system::SyncSystem};

#[tokio::test]
#[rstest]
async fn basic(random_seed: Seed) {
    // TODO: see if this is needed
    let mut _rng = make_seedable_rng(random_seed);

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
        frigate_api_mock.expect_test_call().returning(|| Ok(())); // TODO: test the effect of this failing
    }
    let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);
    let frigate_api_maker = move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone());

    let file_sender_maker = move |pd: &Arc<PathDescriptor>| make_store(pd);

    // TODO: remove the underscore and use this channel
    let (_mqtt_data_sender, mqtt_data_receiver) =
        tokio::sync::mpsc::unbounded_channel::<CapturedPayloads>();

    let (stop_sender, stop_receiver) = tokio::sync::mpsc::unbounded_channel();

    let sync_sys = SyncSystem::new(
        upload_dests,
        Arc::new(frigate_api_config),
        frigate_api_maker,
        file_sender_maker,
        mqtt_data_receiver,
        Some(stop_receiver),
    );

    let task_handle = tokio::task::spawn(async move { sync_sys.start() });

    // NOTE: not all these tests should be done in this unit tests
    // TODO: test changing camera states
    // TODO: create a way to retrieve the current state of cameras
    // TODO: test receiving snapshots with both camera states, on/off
    // TODO: test receiving recordings with both camera states, on/off

    // Shutdown mechanism
    {
        stop_sender.send(()).unwrap();

        task_handle.await.unwrap().await.unwrap();
    }
}
