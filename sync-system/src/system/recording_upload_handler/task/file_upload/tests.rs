use std::{path::Path, sync::Arc};

use crate::config::PathDescriptors;

use super::ReviewUpload;
use file_sender::{
    make_inmemory_filesystem, path_descriptor::PathDescriptor, traits::StoreDestination,
};
use frigate_api_caller::{config::FrigateApiConfig, traits::FrigateApi};
use mocks::{frigate_api::make_frigate_client_mock, store_dest::make_store_mock};
use mqtt_handler::types::reviews::{ReviewProps, payload};
use utils::time_getter::TimeGetter;

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
    file_store_mock.expect_init().returning(|| Ok(())).times(2); // Once for upload and once for alt delete
    file_store_mock
        .expect_mkdir_p()
        .returning(|_| Ok(()))
        .once();
    file_store_mock
        .expect_put_from_memory()
        .returning(|_, _| Ok(()))
        .once();
    file_store_mock
        .expect_file_exists()
        .returning(|_| Ok(false)); // No alt file exists

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
        TimeGetter::default(),
        std::time::Duration::from_millis(500),
    );

    review_upload.start().await.unwrap();
}

#[tokio::test]
async fn basic_upload_in_virtual_filesystem() {
    let frigate_config = FrigateApiConfig {
        frigate_api_base_url: "http://someurl.com:5000/".to_string(),
        frigate_api_proxy: None,
    };

    // Prepare the file sender mock
    let file_sender = make_inmemory_filesystem();
    let file_sender_inner = file_sender.clone();

    let review_new = TestReviewData {
        camera_name: "MyCamera".to_string(),
        start_time: 950.,
        end_time: 1000.,
        id: "id-abcdefg".to_string(),
        type_field: payload::TypeField::New,
    };

    {
        let mut frigate_api_mock = make_frigate_client_mock();

        // Prepare the API mock
        frigate_api_mock
            .expect_recording_clip()
            .returning(|_, _, _| Ok(Some(b"Hello world!".to_vec())));

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

        let mut review_upload = ReviewUpload::new(
            Arc::new(review_new.clone()),
            false,
            Arc::new(frigate_config.clone()),
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,
            TimeGetter::default(),
            std::time::Duration::from_millis(500),
        );

        review_upload.start().await.unwrap();
    }

    // Test the state of the files in the virtual file system
    let dirs = file_sender.ls(Path::new(".")).await.unwrap();
    assert_eq!(dirs.len(), 1);
    assert!(file_sender.dir_exists(&dirs[0]).await.unwrap());

    let uploaded_files_first = file_sender
        .ls(&Path::new(".").join(&dirs[0]))
        .await
        .unwrap();

    assert_eq!(uploaded_files_first.len(), 1);
    assert!(
        uploaded_files_first[0]
            .to_str()
            .unwrap()
            .contains("RecordingClip")
    );
    assert!(
        uploaded_files_first[0]
            .to_str()
            .unwrap()
            .ends_with("-0.mp4")
    );
    assert!(
        uploaded_files_first[0]
            .to_str()
            .unwrap()
            .contains(&review_new.camera_name)
    );

    assert_eq!(
        file_sender
            .get_to_memory(&Path::new(".").join(&dirs[0]).join(&uploaded_files_first[0]))
            .await
            .unwrap(),
        b"Hello world!"
    );

    //////////////////////////////////////////////////////////////////

    let file_sender_inner = file_sender.clone();

    {
        let mut frigate_api_mock = make_frigate_client_mock();

        // Prepare the API mock
        frigate_api_mock
            .expect_recording_clip()
            .returning(|_, _, _| Ok(Some(b"Hello world2!".to_vec())));

        // From the previous run
        assert!(file_sender.ls(Path::new(".")).await.unwrap().len() == 1);

        let frigate_api_mock: Arc<dyn FrigateApi> = Arc::new(frigate_api_mock);

        let frigate_api_maker = Arc::new(move |_: &FrigateApiConfig| Ok(frigate_api_mock.clone()));
        let file_sender_maker =
            Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_sender_inner.clone()));

        let path_descriptors = PathDescriptors {
            path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
                "/home/data/".to_string().into(),
            ))]),
        };

        let mut review_upload = ReviewUpload::new(
            Arc::new(review_new.clone()),
            true,
            Arc::new(frigate_config),
            frigate_api_maker,
            file_sender_maker,
            path_descriptors,
            TimeGetter::default(),
            std::time::Duration::from_millis(500),
        );

        review_upload.start().await.unwrap();
    }

    // Test the state of the files in the virtual file system
    let dirs = file_sender.ls(Path::new(".")).await.unwrap();
    assert_eq!(dirs.len(), 1);
    assert!(file_sender.dir_exists(&dirs[0]).await.unwrap());

    let uploaded_files_second = file_sender
        .ls(&Path::new(".").join(&dirs[0]))
        .await
        .unwrap();

    // There's only one file now because the alternative file was deleted
    assert_eq!(uploaded_files_second.len(), 1);
    assert!(
        uploaded_files_second[0]
            .to_str()
            .unwrap()
            .contains("RecordingClip")
    );
    assert!(
        uploaded_files_second[0]
            .to_str()
            .unwrap()
            .ends_with("-1.mp4")
    );
    assert!(
        uploaded_files_second[0]
            .to_str()
            .unwrap()
            .contains(&review_new.camera_name)
    );

    assert_eq!(
        file_sender
            .get_to_memory(
                &Path::new(".")
                    .join(&dirs[0])
                    .join(&uploaded_files_second[0])
            )
            .await
            .unwrap(),
        b"Hello world2!"
    );
}
