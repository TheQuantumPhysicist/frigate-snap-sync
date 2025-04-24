use std::{path::Path, sync::Arc};

use crate::config::PathDescriptors;

use super::ReviewUpload;
use file_sender::{
    make_inmemory_filesystem, path_descriptor::PathDescriptor, traits::StoreDestination,
};
use frigate_api_caller::{config::FrigateApiConfig, traits::FrigateApi};
use mocks::{frigate_api::make_frigate_client_mock, store_dest::make_store_mock};
use mqtt_handler::types::reviews::{ReviewProps, payload};

#[derive(Debug, Clone)]
struct TestReviewData {
    camera_name: String,
    start_time: f64,
    end_time: f64,
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
        Some(self.end_time)
    }

    fn type_field(&self) -> payload::TypeField {
        self.type_field
    }
}

#[tokio::test]
async fn basic_upload_in_mocks() {
    let mut frigate_api_mock = make_frigate_client_mock();

    // Prepare the API mock
    frigate_api_mock
        .expect_recording_clip()
        .returning(|_, _, _| Ok(Some(b"Hello world!".to_vec())))
        .once();

    // Prepare the file sender mock
    let mut file_store_mock = make_store_mock();
    file_store_mock.expect_init().returning(|| Ok(())).once();
    file_store_mock
        .expect_mkdir_p()
        .returning(|_| Ok(()))
        .once();
    file_store_mock
        .expect_put_from_memory()
        .returning(|_, _| Ok(()))
        .once();

    // Start the testing
    let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);
    let file_store_mock: Arc<dyn StoreDestination<Error = anyhow::Error>> =
        Arc::new(file_store_mock);

    let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));
    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_store_mock.clone()));

    let frigate_config = FrigateApiConfig {
        frigate_api_base_url: "http://someurl.com:5000/".to_string(),
        frigate_api_proxy: None,
    };

    let path_descriptors = PathDescriptors {
        path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
            "/home/data/".to_string().into(),
        ))]),
    };

    let review = TestReviewData {
        camera_name: "MyCamera".to_string(),
        start_time: 950.,
        end_time: 1000.,
        id: "id-abcdefg".to_string(),
        type_field: payload::TypeField::New,
    };

    let mut review_upload = ReviewUpload::new(
        Arc::new(review),
        false,
        Arc::new(frigate_config),
        frigate_api_maker,
        file_sender_maker,
        path_descriptors,
    );

    review_upload.run().await.unwrap();
}

#[tokio::test]
async fn basic_upload_in_virtual_filesystem() {
    let mut frigate_api_mock = make_frigate_client_mock();

    // Prepare the API mock
    frigate_api_mock
        .expect_recording_clip()
        .returning(|_, _, _| Ok(Some(b"Hello world!".to_vec())));

    // Prepare the file sender mock
    let file_sender = make_inmemory_filesystem();
    let file_sender_inner = file_sender.clone();

    // Start testing
    assert!(file_sender.ls(Path::new(".")).await.unwrap().is_empty());

    let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);

    let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));
    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_sender_inner.clone()));

    let frigate_config = FrigateApiConfig {
        frigate_api_base_url: "http://someurl.com:5000/".to_string(),
        frigate_api_proxy: None,
    };

    let path_descriptors = PathDescriptors {
        path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
            "/home/data/".to_string().into(),
        ))]),
    };

    let review = TestReviewData {
        camera_name: "MyCamera".to_string(),
        start_time: 950.,
        end_time: 1000.,
        id: "id-abcdefg".to_string(),
        type_field: payload::TypeField::New,
    };

    let mut review_upload = ReviewUpload::new(
        Arc::new(review.clone()),
        false,
        Arc::new(frigate_config),
        frigate_api_maker,
        file_sender_maker,
        path_descriptors,
    );

    review_upload.run().await.unwrap();

    // Test the state of the files in the virtual file system
    let dirs = file_sender.ls(Path::new(".")).await.unwrap();
    assert_eq!(dirs.len(), 1);
    assert!(file_sender.dir_exists(&dirs[0]).await.unwrap());

    let uploaded_files = file_sender
        .ls(&Path::new(".").join(&dirs[0]))
        .await
        .unwrap();

    assert_eq!(uploaded_files.len(), 1);
    assert!(
        uploaded_files[0]
            .to_str()
            .unwrap()
            .contains("RecordingClip")
    );
    assert!(uploaded_files[0].to_str().unwrap().ends_with("mp4"));
    assert!(
        uploaded_files[0]
            .to_str()
            .unwrap()
            .contains(&review.camera_name)
    );
    assert_eq!(
        file_sender
            .get_to_memory(&Path::new(".").join(&dirs[0]).join(&uploaded_files[0]))
            .await
            .unwrap(),
        b"Hello world!"
    )
}
