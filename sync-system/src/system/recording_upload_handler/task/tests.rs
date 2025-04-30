use super::*;
use crate::system::recording_upload_handler::task::file_upload::MAX_UPLOAD_ATTEMPTS;
use file_sender::{
    make_inmemory_filesystem, path_descriptor::PathDescriptor, traits::StoreDestination,
};
use frigate_api_caller::traits::FrigateApi;
use mocks::{frigate_api::make_frigate_client_mock, store_dest::make_store_mock};
use mqtt_handler::types::reviews::payload;
use rstest::rstest;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use test_utils::{
    asserts::assert_str_ends_with,
    random::{Rng, Seed, gen_random_bytes, make_seedable_rng, random_seed},
};
use utils::time::Time;

const RETRY_PERIOD: std::time::Duration = std::time::Duration::from_millis(500);

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
async fn recording_upload(random_seed: Seed) {
    let mut rng = make_seedable_rng(random_seed);

    let frigate_config = FrigateApiConfig {
        frigate_api_base_url: "http://someurl.com:5000/".to_string(),
        frigate_api_proxy: None,
    };

    let expected_file_content = Arc::new(Mutex::new(gen_random_bytes(&mut rng, 100..1000)));
    let expected_file_content_inner = expected_file_content.clone();

    // Prepare the file sender
    let file_sender = make_inmemory_filesystem();
    let file_sender_inner = file_sender.clone();

    let review_new = TestReviewData {
        camera_name: "MyCamera".to_string(),
        start_time: 950.,
        end_time: None,
        id: "id-abcdefg".to_string(),
        type_field: payload::TypeField::New,
    };

    let expected_dir: PathBuf = Time::from_f64_secs_since_epoch(review_new.start_time)
        .as_local_time_in_dir_foramt()
        .into();

    let (review_sender, review_receiver) = tokio::sync::mpsc::unbounded_channel();

    // Prepare the API mock
    let mut frigate_api_mock = make_frigate_client_mock();
    frigate_api_mock
        .expect_recording_clip()
        .returning(move |_, _, _| {
            Ok(Some(
                expected_file_content_inner.clone().lock().unwrap().clone(),
            ))
        });

    {
        let (first_resolve_sender, first_resolve_receiver) = tokio::sync::oneshot::channel::<()>();
        let (end_sender, end_receiver) = tokio::sync::oneshot::channel::<UploadConclusion>();

        assert!(file_sender.ls(Path::new(".")).await.unwrap().is_empty());

        let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);

        let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));
        let file_sender_maker =
            Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_sender_inner.clone()));

        let path_descriptors = PathDescriptors {
            path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
                "/home/data/".to_string().into(),
            ))]),
        };

        let task = SingleRecordingUploadTask::new(
            Arc::new(review_new),
            Some(first_resolve_sender),
            review_receiver,
            Some(end_sender),
            Arc::new(frigate_config),
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,
            Some(3),
            Some(RETRY_PERIOD),
            TimeGetter::default(),
        );
        let task_handle = tokio::task::spawn(task.start());

        first_resolve_receiver.await.unwrap();

        assert_eq!(file_sender.ls(&expected_dir).await.unwrap().len(), 1);

        let files = file_sender.ls(&expected_dir).await.unwrap();
        let file_name_0 = files[0].to_str().unwrap();
        assert_str_ends_with(file_name_0, "-0.mp4");

        assert_eq!(
            file_sender
                .get_to_memory(&expected_dir.join(Path::new(file_name_0)))
                .await
                .unwrap(),
            *expected_file_content.lock().unwrap()
        );

        {
            // refresh the incoming data, to make sure data is different
            *expected_file_content.lock().unwrap() = gen_random_bytes(&mut rng, 100..1000);

            let review_update_1 = TestReviewData {
                camera_name: "MyCamera".to_string(),
                start_time: 950.,
                end_time: None,
                id: "id-abcdefg".to_string(),
                type_field: payload::TypeField::Update,
            };

            let (review_res_sender, review_res_receiver) = oneshot::channel();

            review_sender
                .send((Arc::new(review_update_1), Some(review_res_sender)))
                .unwrap();

            review_res_receiver.await.unwrap();

            // The other file is deleted, since this one is uploaded successfully
            assert_eq!(
                file_sender
                    .ls(&PathBuf::from(&expected_dir))
                    .await
                    .unwrap()
                    .len(),
                1
            );

            let files = file_sender.ls(&expected_dir).await.unwrap();
            let file_name_1 = files[0].to_str().unwrap();
            assert_str_ends_with(file_name_1, "-1.mp4");

            assert_eq!(
                file_sender
                    .get_to_memory(&expected_dir.join(Path::new(file_name_1)))
                    .await
                    .unwrap(),
                *expected_file_content.lock().unwrap()
            );
        }

        {
            *expected_file_content.lock().unwrap() = gen_random_bytes(&mut rng, 100..1000);

            let review_update_2 = TestReviewData {
                camera_name: "MyCamera".to_string(),
                start_time: 950.,
                end_time: None,
                id: "id-abcdefg".to_string(),
                type_field: payload::TypeField::Update,
            };

            let (review_res_sender, review_res_receiver) = oneshot::channel();

            review_sender
                .send((Arc::new(review_update_2), Some(review_res_sender)))
                .unwrap();

            review_res_receiver.await.unwrap();

            // The other file is deleted, since this one is uploaded successfully
            assert_eq!(
                file_sender
                    .ls(&PathBuf::from(&expected_dir))
                    .await
                    .unwrap()
                    .len(),
                1
            );

            let files = file_sender.ls(&expected_dir).await.unwrap();
            let file_name_0 = files[0].to_str().unwrap();
            assert_str_ends_with(file_name_0, "-0.mp4");

            assert_eq!(
                file_sender
                    .get_to_memory(&expected_dir.join(Path::new(file_name_0)))
                    .await
                    .unwrap(),
                *expected_file_content.lock().unwrap()
            );
        }

        {
            *expected_file_content.lock().unwrap() = gen_random_bytes(&mut rng, 100..1000);

            let review_end = TestReviewData {
                camera_name: "MyCamera".to_string(),
                start_time: 950.,
                end_time: Some(1000.),
                id: "id-abcdefg".to_string(),
                type_field: payload::TypeField::End,
            };

            let (review_res_sender, review_res_receiver) = oneshot::channel();

            review_sender
                .send((Arc::new(review_end), Some(review_res_sender)))
                .unwrap();

            review_res_receiver.await.unwrap();

            // The other file is deleted, since this one is uploaded successfully
            assert_eq!(
                file_sender
                    .ls(&PathBuf::from(&expected_dir))
                    .await
                    .unwrap()
                    .len(),
                1
            );

            let files = file_sender.ls(&expected_dir).await.unwrap();
            let file_name_0 = files[0].to_str().unwrap();
            assert_str_ends_with(file_name_0, "-1.mp4");

            assert_eq!(
                file_sender
                    .get_to_memory(&expected_dir.join(Path::new(file_name_0)))
                    .await
                    .unwrap(),
                *expected_file_content.lock().unwrap()
            );
        }

        task_handle.await.unwrap();

        assert_eq!(end_receiver.await.unwrap(), UploadConclusion::Done);
    }
}

#[tokio::test]
#[rstest]
async fn recording_upload_mocked(random_seed: Seed) {
    let mut rng = make_seedable_rng(random_seed);

    let frigate_config = FrigateApiConfig {
        frigate_api_base_url: "http://someurl.com:5000/".to_string(),
        frigate_api_proxy: None,
    };

    let expected_file_content = Arc::new(Mutex::new(gen_random_bytes(&mut rng, 100..1000)));
    let expected_file_content_inner = expected_file_content.clone();

    // Prepare the file sender mock
    let mut file_store_mock = make_store_mock();
    file_store_mock.expect_init().returning(|| Ok(())).times(8);
    file_store_mock
        .expect_mkdir_p()
        .returning(|_| Ok(()))
        .times(4);
    file_store_mock
        .expect_put_from_memory()
        .returning(|_, _| Ok(()))
        .times(4);

    let file_name = Arc::new(Mutex::new(PathBuf::new()));
    let file_name_clone1 = file_name.clone();
    let file_name_clone2 = file_name.clone();

    file_store_mock
        .expect_file_exists()
        .returning(move |file_name_p| {
            *file_name_clone1.lock().unwrap() = file_name_p.to_owned();
            Ok(true)
        })
        .times(4);
    file_store_mock
        .expect_del_file()
        .returning(move |file_name_p| {
            assert_eq!(file_name_p, &*file_name_clone2.lock().unwrap());
            Ok(())
        })
        .times(4);

    let file_store_mock: Arc<dyn StoreDestination<Error = anyhow::Error>> =
        Arc::new(file_store_mock);

    // Prepare the API mock
    let mut frigate_api_mock = make_frigate_client_mock();
    frigate_api_mock
        .expect_recording_clip()
        .returning(move |_, _, _| {
            Ok(Some(
                expected_file_content_inner.clone().lock().unwrap().clone(),
            ))
        });

    let review_new = TestReviewData {
        camera_name: "MyCamera".to_string(),
        start_time: 950.,
        end_time: None,
        id: "id-abcdefg".to_string(),
        type_field: payload::TypeField::New,
    };

    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_store_mock.clone()));
    let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);
    let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));

    let (review_sender, review_receiver) = tokio::sync::mpsc::unbounded_channel();

    {
        let (first_resolve_sender, first_resolve_receiver) = tokio::sync::oneshot::channel::<()>();
        let (end_sender, end_receiver) = tokio::sync::oneshot::channel::<UploadConclusion>();

        let path_descriptors = PathDescriptors {
            path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
                "/home/data/".to_string().into(),
            ))]),
        };

        let task = SingleRecordingUploadTask::new(
            Arc::new(review_new),
            Some(first_resolve_sender),
            review_receiver,
            Some(end_sender),
            Arc::new(frigate_config),
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,
            Some(3),
            Some(RETRY_PERIOD),
            TimeGetter::default(),
        );
        let task_handle = tokio::task::spawn(task.start());

        first_resolve_receiver.await.unwrap();

        {
            // refresh the incoming data, to make sure data is different
            *expected_file_content.lock().unwrap() = gen_random_bytes(&mut rng, 100..1000);

            let review_update_1 = TestReviewData {
                camera_name: "MyCamera".to_string(),
                start_time: 950.,
                end_time: None,
                id: "id-abcdefg".to_string(),
                type_field: payload::TypeField::Update,
            };

            let (review_res_sender, review_res_receiver) = oneshot::channel();

            review_sender
                .send((Arc::new(review_update_1), Some(review_res_sender)))
                .unwrap();

            review_res_receiver.await.unwrap();
        }

        {
            *expected_file_content.lock().unwrap() = gen_random_bytes(&mut rng, 100..1000);

            let review_update_2 = TestReviewData {
                camera_name: "MyCamera".to_string(),
                start_time: 950.,
                end_time: None,
                id: "id-abcdefg".to_string(),
                type_field: payload::TypeField::Update,
            };

            let (review_res_sender, review_res_receiver) = oneshot::channel();

            review_sender
                .send((Arc::new(review_update_2), Some(review_res_sender)))
                .unwrap();

            review_res_receiver.await.unwrap();
        }

        {
            *expected_file_content.lock().unwrap() = gen_random_bytes(&mut rng, 100..1000);

            let review_end = TestReviewData {
                camera_name: "MyCamera".to_string(),
                start_time: 950.,
                end_time: Some(1000.),
                id: "id-abcdefg".to_string(),
                type_field: payload::TypeField::End,
            };

            let (review_res_sender, review_res_receiver) = oneshot::channel();

            review_sender
                .send((Arc::new(review_end), Some(review_res_sender)))
                .unwrap();

            review_res_receiver.await.unwrap();
        }

        task_handle.await.unwrap();

        assert_eq!(end_receiver.await.unwrap(), UploadConclusion::Done);
    }
}

#[tokio::test]
#[rstest]
async fn recording_upload_mocked_failures_then_success(random_seed: Seed) {
    let mut rng = make_seedable_rng(random_seed);

    let frigate_config = FrigateApiConfig {
        frigate_api_base_url: "http://someurl.com:5000/".to_string(),
        frigate_api_proxy: None,
    };

    let expected_file_content = Arc::new(Mutex::new(gen_random_bytes(&mut rng, 100..1000)));
    let expected_file_content_inner = expected_file_content.clone();

    // Prepare the file sender mock
    let mut file_store_mock = make_store_mock();
    let mut sequence = mockall::Sequence::new();

    // Prepare the API mock
    let mut frigate_api_mock = make_frigate_client_mock();

    // TEST STORY
    // Let's write the story of the test

    // The API failed to give the file twice
    frigate_api_mock
        .expect_recording_clip()
        .returning(move |_, _, _| {
            Err(anyhow::anyhow!(
                "Artificial error when retrieving the video"
            ))
        })
        .once()
        .in_sequence(&mut sequence);
    frigate_api_mock
        .expect_recording_clip()
        .returning(move |_, _, _| {
            Err(anyhow::anyhow!(
                "Artificial error when retrieving the video"
            ))
        })
        .once()
        .in_sequence(&mut sequence);

    // Then it succeeds, and returns a valid file
    frigate_api_mock
        .expect_recording_clip()
        .returning(move |_, _, _| {
            Ok(Some(
                expected_file_content_inner.clone().lock().unwrap().clone(),
            ))
        })
        .once()
        .in_sequence(&mut sequence);

    // After the file is retrieved, we now have the file downloaded, and we init to upload
    file_store_mock
        .expect_init()
        .returning(|| Ok(()))
        .once()
        .in_sequence(&mut sequence);
    file_store_mock
        .expect_mkdir_p()
        .returning(|_| Ok(()))
        .once()
        .in_sequence(&mut sequence);
    // first upload attempt fails
    file_store_mock
        .expect_put_from_memory()
        .returning(|_, _| Err(anyhow::anyhow!("Fake first attempt failure")))
        .once()
        .in_sequence(&mut sequence);

    // This comes from emitting the error
    file_store_mock
        .expect_path_descriptor()
        .return_const(Arc::new(PathDescriptor::Local("<Fake>".to_string().into())))
        .once()
        .in_sequence(&mut sequence);

    // Second upload attempt succeeds
    file_store_mock
        .expect_init()
        .returning(|| Ok(()))
        .once()
        .in_sequence(&mut sequence);
    file_store_mock
        .expect_mkdir_p()
        .returning(|_| Ok(()))
        .once()
        .in_sequence(&mut sequence);
    file_store_mock
        .expect_put_from_memory()
        .returning(|_, _| Ok(()))
        .once()
        .in_sequence(&mut sequence);

    let file_name = Arc::new(Mutex::new(PathBuf::new()));
    let file_name_clone1 = file_name.clone();
    let file_name_clone2 = file_name.clone();

    // If the alternative file is found, we delete it
    file_store_mock
        .expect_init()
        .returning(|| Ok(()))
        .once()
        .in_sequence(&mut sequence);
    file_store_mock
        .expect_file_exists()
        .returning(move |file_name_p| {
            *file_name_clone1.lock().unwrap() = file_name_p.to_owned();
            Ok(true)
        })
        .times(1)
        .in_sequence(&mut sequence);
    file_store_mock
        .expect_del_file()
        .returning(move |file_name_p| {
            assert_eq!(file_name_p, &*file_name_clone2.lock().unwrap());
            Ok(())
        })
        .times(1)
        .in_sequence(&mut sequence);

    let file_store_mock: Arc<dyn StoreDestination<Error = anyhow::Error>> =
        Arc::new(file_store_mock);

    // We start at end immediately to simplify testing errors
    let review_end = TestReviewData {
        camera_name: "MyCamera".to_string(),
        start_time: 950.,
        end_time: None,
        id: "id-abcdefg".to_string(),
        type_field: payload::TypeField::End,
    };

    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_store_mock.clone()));
    let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);
    let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));

    let (review_sender, review_receiver) = tokio::sync::mpsc::unbounded_channel();
    // We only send one review here, no need for sender
    let _review_sender = review_sender;

    {
        let (first_resolve_sender, first_resolve_receiver) = tokio::sync::oneshot::channel::<()>();
        let (end_sender, end_receiver) = tokio::sync::oneshot::channel::<UploadConclusion>();

        let path_descriptors = PathDescriptors {
            path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
                "/home/data/".to_string().into(),
            ))]),
        };

        let task = SingleRecordingUploadTask::new(
            Arc::new(review_end),
            Some(first_resolve_sender),
            review_receiver,
            Some(end_sender),
            Arc::new(frigate_config),
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,
            Some(3),
            Some(RETRY_PERIOD),
            TimeGetter::default(),
        );
        let task_handle = tokio::task::spawn(task.start());

        first_resolve_receiver.await.unwrap();

        task_handle.await.unwrap();

        assert_eq!(end_receiver.await.unwrap(), UploadConclusion::Done);
    }
}

#[tokio::test]
#[rstest]
async fn recording_upload_mocked_failures_return_not_done(random_seed: Seed) {
    let mut rng = make_seedable_rng(random_seed);

    let frigate_config = FrigateApiConfig {
        frigate_api_base_url: "http://someurl.com:5000/".to_string(),
        frigate_api_proxy: None,
    };

    // Prepare the file sender mock
    let file_store_mock = make_store_mock();
    let mut sequence = mockall::Sequence::new();

    // Prepare the API mock
    let mut frigate_api_mock = make_frigate_client_mock();

    let number_of_download_attempts = rng.random_range(3..10);

    // The API failed to give the file twice
    frigate_api_mock
        .expect_recording_clip()
        .returning(move |_, _, _| {
            Err(anyhow::anyhow!(
                "Artificial error when retrieving the video"
            ))
        })
        .once()
        .in_sequence(&mut sequence);

    for _ in 0..number_of_download_attempts {
        frigate_api_mock
            .expect_recording_clip()
            .returning(move |_, _, _| {
                Err(anyhow::anyhow!(
                    "Artificial error when retrieving the video"
                ))
            })
            .once()
            .in_sequence(&mut sequence);
    }

    let file_store_mock: Arc<dyn StoreDestination<Error = anyhow::Error>> =
        Arc::new(file_store_mock);

    // We start at end immediately to simplify testing errors
    let review_end = TestReviewData {
        camera_name: "MyCamera".to_string(),
        start_time: 950.,
        end_time: None,
        id: "id-abcdefg".to_string(),
        type_field: payload::TypeField::End,
    };

    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_store_mock.clone()));
    let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);
    let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));

    let (review_sender, review_receiver) = tokio::sync::mpsc::unbounded_channel();
    // We only send one review here, no need for sender
    let _review_sender = review_sender;

    {
        let (first_resolve_sender, first_resolve_receiver) = tokio::sync::oneshot::channel::<()>();
        let (end_sender, end_receiver) = tokio::sync::oneshot::channel::<UploadConclusion>();

        let path_descriptors = PathDescriptors {
            path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
                "/home/data/".to_string().into(),
            ))]),
        };

        let task = SingleRecordingUploadTask::new(
            Arc::new(review_end),
            Some(first_resolve_sender),
            review_receiver,
            Some(end_sender),
            Arc::new(frigate_config),
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,
            Some(number_of_download_attempts),
            Some(RETRY_PERIOD),
            TimeGetter::default(),
        );
        let task_handle = tokio::task::spawn(task.start());

        first_resolve_receiver.await.unwrap();

        task_handle.await.unwrap();

        assert_eq!(end_receiver.await.unwrap(), UploadConclusion::NotDone);
    }
}

#[tokio::test]
#[rstest]
async fn recording_upload_mocked_failures_in_download_then_upload_leads_to_not_done(
    random_seed: Seed,
) {
    let mut rng = make_seedable_rng(random_seed);

    let frigate_config = FrigateApiConfig {
        frigate_api_base_url: "http://someurl.com:5000/".to_string(),
        frigate_api_proxy: None,
    };

    let expected_file_content = Arc::new(Mutex::new(gen_random_bytes(&mut rng, 100..1000)));
    let expected_file_content_inner = expected_file_content.clone();

    // Prepare the file sender mock
    let mut file_store_mock = make_store_mock();
    let mut sequence = mockall::Sequence::new();

    // Prepare the API mock
    let mut frigate_api_mock = make_frigate_client_mock();

    // TEST STORY
    // Let's write the story of the test

    // The API failed to give the file twice
    frigate_api_mock
        .expect_recording_clip()
        .returning(move |_, _, _| {
            Err(anyhow::anyhow!(
                "Artificial error when retrieving the video"
            ))
        })
        .once()
        .in_sequence(&mut sequence);

    // Then it succeeds, and returns a valid file
    frigate_api_mock
        .expect_recording_clip()
        .returning(move |_, _, _| {
            Ok(Some(
                expected_file_content_inner.clone().lock().unwrap().clone(),
            ))
        })
        .once()
        .in_sequence(&mut sequence);

    // After the file is retrieved, we now have the file downloaded, and we init to upload
    file_store_mock
        .expect_init()
        .returning(|| Ok(()))
        .once()
        .in_sequence(&mut sequence);
    file_store_mock
        .expect_mkdir_p()
        .returning(|_| Ok(()))
        .once()
        .in_sequence(&mut sequence);
    // Upload always fails
    file_store_mock
        .expect_put_from_memory()
        .returning(|_, _| Err(anyhow::anyhow!("Fake first attempt failure")))
        .once()
        .in_sequence(&mut sequence);

    // This comes from emitting the error
    file_store_mock
        .expect_path_descriptor()
        .return_const(Arc::new(PathDescriptor::Local("<Fake>".to_string().into())))
        .once()
        .in_sequence(&mut sequence);

    let number_of_download_attempts: u32 = rng.random_range(4..7);

    for i in 0..number_of_download_attempts * MAX_UPLOAD_ATTEMPTS - 1
    // - 1 for one failure in getting the clip
    {
        file_store_mock
            .expect_init()
            .returning(|| Ok(()))
            .once()
            .in_sequence(&mut sequence);
        file_store_mock
            .expect_mkdir_p()
            .returning(|_| Ok(()))
            .once()
            .in_sequence(&mut sequence);
        file_store_mock
            .expect_put_from_memory()
            .returning(move |_, _| Err(anyhow::anyhow!("Fake attempt {i} failure")))
            .once()
            .in_sequence(&mut sequence);
        file_store_mock
            .expect_path_descriptor()
            .return_const(Arc::new(PathDescriptor::Local("<Fake>".to_string().into())))
            .once()
            .in_sequence(&mut sequence);
    }

    let file_store_mock: Arc<dyn StoreDestination<Error = anyhow::Error>> =
        Arc::new(file_store_mock);

    // We start at end immediately to simplify testing errors
    let review_end = TestReviewData {
        camera_name: "MyCamera".to_string(),
        start_time: 950.,
        end_time: None,
        id: "id-abcdefg".to_string(),
        type_field: payload::TypeField::End,
    };

    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_store_mock.clone()));
    let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);
    let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));

    let (review_sender, review_receiver) = tokio::sync::mpsc::unbounded_channel();
    // We only send one review here, no need for sender
    let _review_sender = review_sender;

    {
        let (first_resolve_sender, first_resolve_receiver) = tokio::sync::oneshot::channel::<()>();
        let (end_sender, end_receiver) = tokio::sync::oneshot::channel::<UploadConclusion>();

        let path_descriptors = PathDescriptors {
            path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
                "/home/data/".to_string().into(),
            ))]),
        };

        let task = SingleRecordingUploadTask::new(
            Arc::new(review_end),
            Some(first_resolve_sender),
            review_receiver,
            Some(end_sender),
            Arc::new(frigate_config),
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,
            Some(number_of_download_attempts),
            Some(RETRY_PERIOD),
            TimeGetter::default(),
        );
        let task_handle = tokio::task::spawn(task.start());

        first_resolve_receiver.await.unwrap();

        task_handle.await.unwrap();

        assert_eq!(end_receiver.await.unwrap(), UploadConclusion::NotDone);
    }
}
