use crate::{config::PathDescriptors, state::CamerasState, system::SyncSystem};
use file_sender::{make_store, path_descriptor::PathDescriptor};
use frigate_api_caller::{config::FrigateApiConfig, json::stats::StatsProps, traits::FrigateApi};
use mocks::frigate_api::make_frigate_client_mock;
use mqtt_handler::types::{
    CapturedPayloads,
    reviews::{ReviewProps, payload},
    snapshot::Snapshot,
    snapshots_state::SnapshotsState,
};
use rstest::rstest;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicU64},
};
use test_utils::{asserts::assert_slice_contains, random::Rng};
use test_utils::{
    asserts::{assert_str_contains, assert_str_starts_with},
    random::{Seed, gen_random_bytes, gen_random_string, make_seedable_rng, random_seed},
};
use tokio::sync::{mpsc::UnboundedSender, oneshot};

const VERY_LONG_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

struct TestStats {
    uptime: std::time::Duration,
}

impl StatsProps for TestStats {
    fn uptime(&self) -> std::time::Duration {
        self.uptime
    }
}

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
#[trace]
async fn basic_syncsystem_uploads(
    random_seed: Seed,
    #[values(false, true)] pass_initial_api_test: bool,
) {
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
        delay_after_startup: std::time::Duration::ZERO,
    };

    let mut frigate_api_mock = make_frigate_client_mock();
    let frigate_returned_video_data_vec = b"012345".to_vec();
    {
        frigate_api_mock.expect_test_call().returning(move || {
            // Passing initial API call tests shouldn't matter. It's just to inform the user in logs.
            if pass_initial_api_test {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Fake api error for tests"))
            }
        });
        frigate_api_mock.expect_stats().returning(|| {
            Ok(Box::new(TestStats {
                uptime: std::time::Duration::from_secs(10000),
            }))
        });
        frigate_api_mock
            .expect_recording_clip()
            .returning(move |_, _, _| Ok(Some(frigate_returned_video_data_vec.clone())));
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

    // Start with an empty state
    {
        let camera_state = get_camera_state(&camera_state_getter_sender).await;
        assert!(camera_state.recordings_state().is_empty());
        assert!(camera_state.snapshots_state().is_empty());
    }

    // Mqtt sends a snapshot, but state is disabled, so no upload
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

    // Mqtt sends a recording, but state is disabled, so no upload
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

    // Send a command to enable snapshots
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
                tokio::time::timeout(VERY_LONG_WAIT, async {
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

    // Send a snapshot from mqtt, now it should work and upload after snapshots are enabled
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
                tokio::time::timeout(VERY_LONG_WAIT, async {
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

            // Upload directory
            let dirs = file_sender.ls(Path::new(".")).await.unwrap();
            assert_eq!(dirs.len(), 1);
            // Expect one file
            let files = file_sender.ls(&dirs[0]).await.unwrap();
            assert_eq!(files.len(), 1);
            assert_str_starts_with(&files[0].display().to_string(), "Snapshot");
            assert_str_contains(&files[0].display().to_string(), camera1_label);
        }
    }

    // Send a command to enable recordings
    {
        {
            let camera_state = get_camera_state(&camera_state_getter_sender).await;
            assert!(camera_state.recordings_state().is_empty());
            assert_eq!(camera_state.snapshots_state().len(), 1);
        }

        {
            let enable_payload = CapturedPayloads::CameraRecordingsState(
                mqtt_handler::types::recordings_state::RecordingsState {
                    camera_label: camera1_label.to_string(),
                    state: true,
                },
            );
            mqtt_data_sender.send(enable_payload).unwrap();
        }

        {
            {
                // We can't guarantee that the mqtt state update will happen in order, so we just wait for it for a while
                tokio::time::timeout(VERY_LONG_WAIT, async {
                    loop {
                        if !get_camera_state(&camera_state_getter_sender)
                            .await
                            .recordings_state()
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
            assert_eq!(camera_state.recordings_state().len(), 1);
        }
    }

    // Upload a recording, should work after we enabled it
    {
        let review = TestReviewData {
            camera_name: camera1_label.to_string(),
            start_time: 950.,
            end_time: None,
            id: "id-abcdefg".to_string(),
            type_field: payload::TypeField::End, // We use end because otherwise the upload task is considered unfinished
        };
        let payload = CapturedPayloads::Reviews(Arc::new(review));
        mqtt_data_sender.send(payload).unwrap();

        for pd in &*upload_dests.path_descriptors {
            let file_sender = file_sender_maker(pd).unwrap();

            {
                // We can't guarantee that the upload will happen before we check, so we gotta wait for it
                tokio::time::timeout(VERY_LONG_WAIT, async {
                    loop {
                        let dirs = file_sender.ls(Path::new(".")).await.unwrap();
                        if dirs.len() == 2
                            && !file_sender.ls(&dirs[0]).await.unwrap().is_empty()
                            && !file_sender.ls(&dirs[1]).await.unwrap().is_empty()
                        {
                            break;
                        }
                    }
                    futures::future::ready(()).await;
                })
                .await
                .unwrap();
            }

            // Upload directory - we expect directory from 01-01-1970 due to a very early timestamp
            let dirs_in = file_sender.ls(Path::new(".")).await.unwrap();
            let expected_dir = PathBuf::from("1970-01-01");
            assert_slice_contains(&dirs_in, &expected_dir);
            // Expect one file
            let files = file_sender.ls(&expected_dir).await.unwrap();
            assert_eq!(files.len(), 1);
            assert_str_starts_with(&files[0].display().to_string(), "RecordingClip");
            assert_str_contains(&files[0].display().to_string(), camera1_label);
        }
    }

    // Shutdown mechanism
    {
        stop_sender.send(()).unwrap();

        tokio::time::timeout(VERY_LONG_WAIT, task_handle)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
#[rstest]
#[trace]
async fn basic_syncsystem_uploads_with_delay_test(
    random_seed: Seed,
    #[values(false, true)] pass_initial_api_test: bool,
) {
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

    // Uptime is an Arc atomic to be easily passed and modified in tests
    let frigate_uptime_value = Arc::new(AtomicU64::new(0));

    let frigate_uptime = {
        let uptime_inner = frigate_uptime_value.clone();
        Arc::new(move || {
            std::time::Duration::from_secs(uptime_inner.load(std::sync::atomic::Ordering::SeqCst))
        })
    };
    let set_frigate_uptime = {
        let uptime_inner = frigate_uptime_value.clone();
        move |t: std::time::Duration| {
            uptime_inner.store(t.as_secs(), std::sync::atomic::Ordering::SeqCst);
        }
    };
    let delay_after_startup = std::time::Duration::from_secs(rng.random_range(1..1000));

    let frigate_api_config = FrigateApiConfig {
        frigate_api_base_url: "http://example.com".to_string(),
        frigate_api_proxy: None,
        delay_after_startup,
    };

    let mut frigate_api_mock = make_frigate_client_mock();
    let frigate_returned_video_data_vec = b"012345".to_vec();
    {
        frigate_api_mock.expect_test_call().returning(move || {
            // Passing initial API call tests shouldn't matter. It's just to inform the user in logs.
            if pass_initial_api_test {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Fake api error for tests"))
            }
        });
        let frigate_uptime_f_inner = frigate_uptime.clone();
        frigate_api_mock.expect_stats().returning(move || {
            Ok(Box::new(TestStats {
                uptime: frigate_uptime_f_inner(),
            }))
        });
        frigate_api_mock
            .expect_recording_clip()
            .returning(move |_, _, _| Ok(Some(frigate_returned_video_data_vec.clone())));
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

    // Start with an empty state
    {
        let camera_state = get_camera_state(&camera_state_getter_sender).await;
        assert!(camera_state.recordings_state().is_empty());
        assert!(camera_state.snapshots_state().is_empty());
    }

    let camera1_label = "camera1_label";

    // Enable snapshots uploading
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
                tokio::time::timeout(VERY_LONG_WAIT, async {
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

    // Send a command to enable recordings
    {
        {
            let camera_state = get_camera_state(&camera_state_getter_sender).await;
            assert!(camera_state.recordings_state().is_empty());
            assert_eq!(camera_state.snapshots_state().len(), 1);
        }

        {
            let enable_payload = CapturedPayloads::CameraRecordingsState(
                mqtt_handler::types::recordings_state::RecordingsState {
                    camera_label: camera1_label.to_string(),
                    state: true,
                },
            );
            mqtt_data_sender.send(enable_payload).unwrap();
        }

        {
            {
                // We can't guarantee that the mqtt state update will happen in order, so we just wait for it for a while
                tokio::time::timeout(VERY_LONG_WAIT, async {
                    loop {
                        if !get_camera_state(&camera_state_getter_sender)
                            .await
                            .recordings_state()
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
            assert_eq!(camera_state.recordings_state().len(), 1);
        }
    }

    // No upload will happen now because uptime is lower than the requested delay after startup
    assert!(frigate_uptime() < delay_after_startup);
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

    // Mqtt sends a recording, but uptime not reached, so no upload
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

    // Set required delay to be the required delay + 1 second
    set_frigate_uptime(
        delay_after_startup + std::time::Duration::from_secs(rng.random_range(0..100)),
    );

    // Send a snapshot from mqtt, now it should work since uptime >= delay_after_startup
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
                tokio::time::timeout(VERY_LONG_WAIT, async {
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

            // Upload directory
            let dirs = file_sender.ls(Path::new(".")).await.unwrap();
            assert_eq!(dirs.len(), 1);
            // Expect one file
            let files = file_sender.ls(&dirs[0]).await.unwrap();
            assert_eq!(files.len(), 1);
            assert_str_starts_with(&files[0].display().to_string(), "Snapshot");
            assert_str_contains(&files[0].display().to_string(), camera1_label);
        }
    }

    // Upload a recording, now it should work since uptime >= delay_after_startup
    {
        let review = TestReviewData {
            camera_name: camera1_label.to_string(),
            start_time: 950.,
            end_time: None,
            id: "id-abcdefg".to_string(),
            type_field: payload::TypeField::End, // We use end because otherwise the upload task is considered unfinished
        };
        let payload = CapturedPayloads::Reviews(Arc::new(review));
        mqtt_data_sender.send(payload).unwrap();

        for pd in &*upload_dests.path_descriptors {
            let file_sender = file_sender_maker(pd).unwrap();

            {
                // We can't guarantee that the upload will happen before we check, so we gotta wait for it
                tokio::time::timeout(VERY_LONG_WAIT, async {
                    loop {
                        let dirs = file_sender.ls(Path::new(".")).await.unwrap();
                        if dirs.len() == 2
                            && !file_sender.ls(&dirs[0]).await.unwrap().is_empty()
                            && !file_sender.ls(&dirs[1]).await.unwrap().is_empty()
                        {
                            break;
                        }
                    }
                    futures::future::ready(()).await;
                })
                .await
                .unwrap();
            }

            // Upload directory - we expect directory from 01-01-1970 due to a very early timestamp
            let dirs_in = file_sender.ls(Path::new(".")).await.unwrap();
            let expected_dir = PathBuf::from("1970-01-01");
            assert_slice_contains(&dirs_in, &expected_dir);
            // Expect one file
            let files = file_sender.ls(&expected_dir).await.unwrap();
            assert_eq!(files.len(), 1);
            assert_str_starts_with(&files[0].display().to_string(), "RecordingClip");
            assert_str_contains(&files[0].display().to_string(), camera1_label);
        }
    }

    // Shutdown mechanism
    {
        stop_sender.send(()).unwrap();

        tokio::time::timeout(VERY_LONG_WAIT, task_handle)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
