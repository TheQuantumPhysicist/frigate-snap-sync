use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use file_sender::{make_inmemory_filesystem, path_descriptor::PathDescriptor};
use frigate_api_caller::traits::FrigateApi;
use mocks::frigate_api::make_frigate_client_mock;
use mqtt_handler::types::reviews::payload;
use rstest::rstest;
use test_utils::{
    asserts::assert_str_ends_with,
    random::{Seed, gen_random_bytes, make_seedable_rng, random_seed},
};
use utils::time::Time;

use super::*;

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

    // Prepare the file sender mock
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
            Some(std::time::Duration::from_secs(2)),
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
