use super::*;
use file_sender::{
    make_inmemory_filesystem, path_descriptor::PathDescriptor, traits::StoreDestination,
};
use mocks::store_dest::make_store_mock;
use rstest::rstest;
use std::path::Path;
use test_utils::{
    asserts::assert_str_contains,
    random::{Seed, gen_random_bytes, make_seedable_rng, random_seed},
};

const VERY_LONG_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

async fn get_task_count(
    cmd_sender: &tokio::sync::mpsc::UnboundedSender<SnapshotsUploadTaskHandlerCommand>,
) -> usize {
    let (size_sender, size_receiver) = oneshot::channel();

    cmd_sender
        .send(SnapshotsUploadTaskHandlerCommand::GetTaskCount(size_sender))
        .unwrap();

    size_receiver.await.unwrap()
}

#[tokio::test]
#[rstest]
async fn upload_snapshot(random_seed: Seed) {
    let mut rng = make_seedable_rng(random_seed);

    let (cmd_sender, cmd_receiver) = tokio::sync::mpsc::unbounded_channel();

    // Prepare the file sender
    let file_sender = make_inmemory_filesystem();
    let file_sender_inner = file_sender.clone();

    let path_descriptors = PathDescriptors {
        path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
            "/home/data/".to_string().into(),
        ))]),
    };

    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_sender_inner.clone()));

    let task_handler = SnapshotsTaskHandler::new(cmd_receiver, file_sender_maker, path_descriptors);

    let task_handle = tokio::task::spawn(task_handler.run());

    let image_bytes = gen_random_bytes(&mut rng, 100..200);

    assert_eq!(file_sender.ls(Path::new(".")).await.unwrap().len(), 0);

    {
        let snapshot = Snapshot {
            image_bytes,
            camera_label: "CameraLabel".to_string(),
            object_name: "Snapshot1".to_string(),
        };

        let (confirm_sender, confirm_receiver) = oneshot::channel();

        let snapshot = Arc::new(snapshot);

        cmd_sender
            .send(SnapshotsUploadTaskHandlerCommand::Task(
                snapshot.clone(),
                Some(confirm_sender),
            ))
            .unwrap();

        confirm_receiver.await.unwrap();

        // Wait for the task/upload to finish
        tokio::time::timeout(VERY_LONG_WAIT, async {
            if get_task_count(&cmd_sender).await > 0 {
                futures::future::pending::<()>().await
            } else {
                futures::future::ready(()).await
            }
        })
        .await
        .unwrap();

        // Assert the file is uploaded with the expected file name into the virtual filesystem
        assert_eq!(file_sender.ls(Path::new(".")).await.unwrap().len(), 1);

        let dir_name = &file_sender.ls(Path::new(".")).await.unwrap()[0];

        assert_str_contains(
            file_sender.ls(dir_name).await.unwrap()[0].to_str().unwrap(),
            &snapshot.camera_label,
        );
        assert_str_contains(
            file_sender.ls(dir_name).await.unwrap()[0].to_str().unwrap(),
            &snapshot.object_name,
        );
    }

    // stop and shutdown
    {
        cmd_sender
            .send(SnapshotsUploadTaskHandlerCommand::Stop)
            .unwrap();

        task_handle.await.unwrap();
    }
}

#[tokio::test]
#[rstest]
async fn upload_snapshot_mocked(random_seed: Seed) {
    let mut rng = make_seedable_rng(random_seed);

    let (cmd_sender, cmd_receiver) = tokio::sync::mpsc::unbounded_channel();

    // Prepare the file sender
    let mut file_store_mock = make_store_mock();
    let mut seq = mockall::Sequence::new();

    file_store_mock
        .expect_init()
        .once()
        .returning(|| Ok(()))
        .in_sequence(&mut seq);
    file_store_mock
        .expect_mkdir_p()
        .once()
        .returning(|_| Ok(()))
        .in_sequence(&mut seq);
    file_store_mock
        .expect_put_from_memory()
        .once()
        .returning(|_, _| Ok(()))
        .in_sequence(&mut seq);

    let file_store_mock: Arc<dyn StoreDestination<Error = anyhow::Error>> =
        Arc::new(file_store_mock);

    let path_descriptors = PathDescriptors {
        path_descriptors: Arc::new(vec![Arc::new(PathDescriptor::Local(
            "/home/data/".to_string().into(),
        ))]),
    };

    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_store_mock.clone()));

    let task_handler = SnapshotsTaskHandler::new(cmd_receiver, file_sender_maker, path_descriptors);

    let task_handle = tokio::task::spawn(task_handler.run());

    let image_bytes = gen_random_bytes(&mut rng, 100..200);

    {
        let snapshot = Snapshot {
            image_bytes,
            camera_label: "CameraLabel".to_string(),
            object_name: "Snapshot1".to_string(),
        };

        let (confirm_sender, confirm_receiver) = oneshot::channel();

        let snapshot = Arc::new(snapshot);

        cmd_sender
            .send(SnapshotsUploadTaskHandlerCommand::Task(
                snapshot.clone(),
                Some(confirm_sender),
            ))
            .unwrap();

        confirm_receiver.await.unwrap();

        // Wait for the task/upload to finish
        tokio::time::timeout(VERY_LONG_WAIT, async {
            if get_task_count(&cmd_sender).await > 0 {
                futures::future::pending::<()>().await
            } else {
                futures::future::ready(()).await
            }
        })
        .await
        .unwrap();
    }

    // stop and shutdown
    {
        cmd_sender
            .send(SnapshotsUploadTaskHandlerCommand::Stop)
            .unwrap();

        task_handle.await.unwrap();
    }
}

#[tokio::test]
#[rstest]
async fn upload_snapshot_mocked_error_mkdir(random_seed: Seed) {
    let mut rng = make_seedable_rng(random_seed);

    let (cmd_sender, cmd_receiver) = tokio::sync::mpsc::unbounded_channel();

    let path_descriptors = Arc::new(vec![Arc::new(PathDescriptor::Local(
        "/home/data/".to_string().into(),
    ))]);
    let path_descriptors = PathDescriptors { path_descriptors };

    // Prepare the file sender
    let mut file_store_mock = make_store_mock();
    let mut seq = mockall::Sequence::new();

    file_store_mock
        .expect_init()
        .once()
        .returning(|| Ok(()))
        .in_sequence(&mut seq);
    file_store_mock
        .expect_mkdir_p()
        .once()
        .returning(|_| Err(anyhow::anyhow!("Faked error in mkdir")))
        .in_sequence(&mut seq);
    file_store_mock
        .expect_path_descriptor()
        .once()
        .return_const(path_descriptors.path_descriptors[0].clone())
        .in_sequence(&mut seq);

    // After this failure, it'll try again later
    file_store_mock
        .expect_init()
        .once()
        .returning(|| Ok(()))
        .in_sequence(&mut seq);
    file_store_mock
        .expect_mkdir_p()
        .once()
        .returning(|_| Ok(()))
        .in_sequence(&mut seq);
    file_store_mock
        .expect_put_from_memory()
        .once()
        .returning(|_, _| Ok(()))
        .in_sequence(&mut seq);

    let file_store_mock: Arc<dyn StoreDestination<Error = anyhow::Error>> =
        Arc::new(file_store_mock);

    let file_sender_maker = Arc::new(move |_: &Arc<PathDescriptor>| Ok(file_store_mock.clone()));

    let task_handler = SnapshotsTaskHandler::new(cmd_receiver, file_sender_maker, path_descriptors);

    let task_handle = tokio::task::spawn(task_handler.run());

    let image_bytes = gen_random_bytes(&mut rng, 100..200);

    {
        let snapshot = Snapshot {
            image_bytes,
            camera_label: "CameraLabel".to_string(),
            object_name: "Snapshot1".to_string(),
        };

        let (confirm_sender, confirm_receiver) = oneshot::channel();

        let snapshot = Arc::new(snapshot);

        cmd_sender
            .send(SnapshotsUploadTaskHandlerCommand::Task(
                snapshot.clone(),
                Some(confirm_sender),
            ))
            .unwrap();

        confirm_receiver.await.unwrap();

        // Wait for the task/upload to finish
        tokio::time::timeout(VERY_LONG_WAIT, async {
            if get_task_count(&cmd_sender).await > 0 {
                futures::future::pending::<()>().await
            } else {
                futures::future::ready(()).await
            }
        })
        .await
        .unwrap();
    }

    // stop and shutdown
    {
        cmd_sender
            .send(SnapshotsUploadTaskHandlerCommand::Stop)
            .unwrap();

        task_handle.await.unwrap();
    }
}
