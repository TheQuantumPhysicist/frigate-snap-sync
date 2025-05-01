use super::RecordingsTaskHandler;
use crate::{
    config::PathDescriptors, system::recording_upload_handler::RecordingsUploadTaskHandlerCommand,
};
use file_sender::{make_inmemory_filesystem, path_descriptor::PathDescriptor};
use frigate_api_caller::{config::FrigateApiConfig, traits::FrigateApi};
use mocks::frigate_api::make_frigate_client_mock;
use mqtt_handler::types::reviews::{ReviewProps, payload};
use rstest::rstest;
use std::sync::{Arc, Mutex};
use test_utils::random::{Seed, gen_random_bytes, make_seedable_rng, random_seed};
use tokio::sync::oneshot;

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

async fn get_task_count(
    cmd_sender: &tokio::sync::mpsc::UnboundedSender<RecordingsUploadTaskHandlerCommand>,
) -> usize {
    let (size_sender, size_receiver) = oneshot::channel();

    cmd_sender
        .send(RecordingsUploadTaskHandlerCommand::GetTaskCount(
            size_sender,
        ))
        .unwrap();

    size_receiver.await.unwrap()
}

async fn assert_not_finished_for(
    handle: &tokio::task::JoinHandle<impl Send + 'static>,
    duration: std::time::Duration,
) {
    let start = tokio::time::Instant::now();
    while start.elapsed() < duration {
        assert!(!handle.is_finished(), "Task finished too early!");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

#[tokio::test]
#[rstest]
async fn recordings_task_handler(random_seed: Seed) {
    let mut rng = make_seedable_rng(random_seed);

    let (cmd_sender, cmd_receiver) = tokio::sync::mpsc::unbounded_channel();

    let frigate_config = FrigateApiConfig {
        frigate_api_base_url: "http://someurl.com:5000/".to_string(),
        frigate_api_proxy: None,
    };

    let path_descriptors = PathDescriptors {
        path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
            "/home/data/".to_string().into(),
        ))]),
    };

    let expected_file_content = Arc::new(Mutex::new(gen_random_bytes(&mut rng, 100..1000)));
    let expected_file_content_inner = expected_file_content.clone();

    // Prepare the file sender
    let file_sender = make_inmemory_filesystem();

    // Prepare the API mock
    let mut frigate_api_mock = make_frigate_client_mock();
    frigate_api_mock
        .expect_recording_clip()
        .returning(move |_, _, _| {
            Ok(Some(
                expected_file_content_inner.clone().lock().unwrap().clone(),
            ))
        });
    let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);
    let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));

    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_sender.clone()));

    let task = RecordingsTaskHandler::new(
        cmd_receiver,
        Arc::new(frigate_config),
        frigate_api_maker,
        file_sender_maker,
        path_descriptors,
        None,
        None,
    );

    let task_handle = tokio::task::spawn(task.run());

    assert_eq!(get_task_count(&cmd_sender).await, 0);

    {
        let review_new = TestReviewData {
            camera_name: "MyCamera".to_string(),
            start_time: 950.,
            end_time: None,
            id: "id-abcdefg".to_string(),
            type_field: payload::TypeField::New,
        };

        let (confirm_sender, confirm_receiver) = oneshot::channel();

        cmd_sender
            .send(RecordingsUploadTaskHandlerCommand::Task(
                Arc::new(review_new),
                Some(confirm_sender),
            ))
            .unwrap();

        confirm_receiver.await.unwrap();

        assert_eq!(get_task_count(&cmd_sender).await, 1);
    }

    {
        let review_update_1 = TestReviewData {
            camera_name: "MyCamera".to_string(),
            start_time: 950.,
            end_time: None,
            id: "id-abcdefg".to_string(),
            type_field: payload::TypeField::Update,
        };

        let (confirm_sender, confirm_receiver) = oneshot::channel();

        cmd_sender
            .send(RecordingsUploadTaskHandlerCommand::Task(
                Arc::new(review_update_1),
                Some(confirm_sender),
            ))
            .unwrap();

        confirm_receiver.await.unwrap();

        assert_eq!(get_task_count(&cmd_sender).await, 1);
    }

    {
        let review_update_2 = TestReviewData {
            camera_name: "MyCamera".to_string(),
            start_time: 950.,
            end_time: None,
            id: "id-abcdefg".to_string(),
            type_field: payload::TypeField::Update,
        };

        let (confirm_sender, confirm_receiver) = oneshot::channel();

        cmd_sender
            .send(RecordingsUploadTaskHandlerCommand::Task(
                Arc::new(review_update_2),
                Some(confirm_sender),
            ))
            .unwrap();

        confirm_receiver.await.unwrap();

        assert_eq!(get_task_count(&cmd_sender).await, 1);
    }

    {
        let review_end = TestReviewData {
            camera_name: "MyCamera".to_string(),
            start_time: 950.,
            end_time: Some(1000.),
            id: "id-abcdefg".to_string(),
            type_field: payload::TypeField::End,
        };

        let (confirm_sender, confirm_receiver) = oneshot::channel();

        cmd_sender
            .send(RecordingsUploadTaskHandlerCommand::Task(
                Arc::new(review_end),
                Some(confirm_sender),
            ))
            .unwrap();

        confirm_receiver.await.unwrap();

        // Now after the end event, the task should be evicted
        assert_eq!(get_task_count(&cmd_sender).await, 0);
    }

    // stop and shutdown
    {
        cmd_sender
            .send(RecordingsUploadTaskHandlerCommand::Stop)
            .unwrap();

        task_handle.await.unwrap();
    }
}

#[tokio::test]
#[rstest]
async fn recordings_task_handler_shutdown(random_seed: Seed) {
    let mut rng = make_seedable_rng(random_seed);

    let (cmd_sender, cmd_receiver) = tokio::sync::mpsc::unbounded_channel();

    let frigate_config = FrigateApiConfig {
        frigate_api_base_url: "http://someurl.com:5000/".to_string(),
        frigate_api_proxy: None,
    };

    let path_descriptors = PathDescriptors {
        path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
            "/home/data/".to_string().into(),
        ))]),
    };

    let expected_file_content = Arc::new(Mutex::new(gen_random_bytes(&mut rng, 100..1000)));
    let expected_file_content_inner = expected_file_content.clone();

    // Prepare the file sender
    let file_sender = make_inmemory_filesystem();
    let file_sender_inner = file_sender.clone();

    // Prepare the API mock
    let mut frigate_api_mock = make_frigate_client_mock();
    frigate_api_mock
        .expect_recording_clip()
        .returning(move |_, _, _| {
            Ok(Some(
                expected_file_content_inner.clone().lock().unwrap().clone(),
            ))
        });
    let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);
    let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));

    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_sender_inner.clone()));

    let task = RecordingsTaskHandler::new(
        cmd_receiver,
        Arc::new(frigate_config),
        frigate_api_maker,
        file_sender_maker,
        path_descriptors,
        None,
        None,
    );

    let task_handle = tokio::task::spawn(task.run());

    assert_eq!(get_task_count(&cmd_sender).await, 0);

    let wait_time = std::time::Duration::from_secs(5);

    // Without the stop signal, it won't stop
    {
        assert_not_finished_for(&task_handle, wait_time).await;
        assert!(!task_handle.is_finished());
    }

    // stop and shutdown
    {
        cmd_sender
            .send(RecordingsUploadTaskHandlerCommand::Stop)
            .unwrap();

        task_handle.await.unwrap();
    }
}

#[tokio::test]
#[rstest]
async fn recordings_task_handler_timeout_loses_task(random_seed: Seed) {
    let mut rng = make_seedable_rng(random_seed);

    let (cmd_sender, cmd_receiver) = tokio::sync::mpsc::unbounded_channel();

    let frigate_config = FrigateApiConfig {
        frigate_api_base_url: "http://someurl.com:5000/".to_string(),
        frigate_api_proxy: None,
    };

    let path_descriptors = PathDescriptors {
        path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
            "/home/data/".to_string().into(),
        ))]),
    };

    let expected_file_content = Arc::new(Mutex::new(gen_random_bytes(&mut rng, 100..1000)));
    let expected_file_content_inner = expected_file_content.clone();

    // Prepare the file sender
    let file_sender = make_inmemory_filesystem();
    let file_sender_inner = file_sender.clone();

    // Prepare the API mock
    let mut frigate_api_mock = make_frigate_client_mock();
    frigate_api_mock
        .expect_recording_clip()
        .returning(move |_, _, _| {
            Ok(Some(
                expected_file_content_inner.clone().lock().unwrap().clone(),
            ))
        });
    let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);
    let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));

    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_sender_inner.clone()));

    let max_retries = 3;
    let retry_period = std::time::Duration::from_millis(500);
    let total_wait_period = 2 * max_retries * retry_period; // multiply by 2 for safety

    let task = RecordingsTaskHandler::new(
        cmd_receiver,
        Arc::new(frigate_config),
        frigate_api_maker,
        file_sender_maker,
        path_descriptors,
        Some(max_retries),
        Some(retry_period),
    );

    let task_handle = tokio::task::spawn(task.run());

    assert_eq!(get_task_count(&cmd_sender).await, 0);

    {
        let review_new = TestReviewData {
            camera_name: "MyCamera".to_string(),
            start_time: 950.,
            end_time: None,
            id: "id-abcdefg".to_string(),
            type_field: payload::TypeField::New,
        };

        let (confirm_sender, confirm_receiver) = oneshot::channel();

        cmd_sender
            .send(RecordingsUploadTaskHandlerCommand::Task(
                Arc::new(review_new),
                Some(confirm_sender),
            ))
            .unwrap();

        confirm_receiver.await.unwrap();

        assert_eq!(get_task_count(&cmd_sender).await, 1);
    }

    {
        assert_eq!(get_task_count(&cmd_sender).await, 1);
        // We wait for the task to fail
        tokio::time::sleep(total_wait_period).await;
        // After waiting long enough, the task should be dead
        assert_eq!(get_task_count(&cmd_sender).await, 0);
    }

    // stop and shutdown
    {
        cmd_sender
            .send(RecordingsUploadTaskHandlerCommand::Stop)
            .unwrap();

        task_handle.await.unwrap();
    }
}
