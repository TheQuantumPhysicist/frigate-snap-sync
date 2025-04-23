use crate::{
    make_inmemory_filesystem, make_store, path_descriptor::PathDescriptor, traits::StoreDestination,
};
use rstest::rstest;
use std::{
    fmt::{Debug, Display},
    path::{Path, PathBuf},
    sync::Arc,
};
use test_utils::random::{
    Rng, Seed, gen_random_bytes, gen_random_string, make_seedable_rng, random_seed,
};

async fn test_store<E: Display + Debug, S: StoreDestination<Error = E> + ?Sized>(
    fs: &S,
    rng: &mut impl Rng,
) {
    assert!(fs.ls(Path::new(".")).await.unwrap().is_empty());

    // Test that random files and directory names don't exist
    for _ in 0..10 {
        let name = gen_random_string(rng, 10..=50);
        assert!(!fs.file_exists(Path::new(&format!("{name}"))).await.unwrap());
        assert!(!fs.dir_exists(Path::new(&format!("{name}"))).await.unwrap());
    }

    // Test writing file from memory
    {
        let bytes = gen_random_bytes(rng, 100..1000);
        let file_name: PathBuf = gen_random_string(rng, 10..20).into();

        fs.put_from_memory(&bytes, &file_name).await.unwrap();

        let bytes_read = fs.get_to_memory(&file_name).await.unwrap();
        assert_eq!(bytes_read, bytes);

        assert!(fs.file_exists(&file_name).await.unwrap());
        assert_eq!(fs.ls(Path::new(".")).await.unwrap(), [file_name.clone()]);
        fs.del_file(&file_name).await.unwrap();
        assert!(!fs.file_exists(&file_name).await.unwrap());
        assert_eq!(fs.ls(Path::new(".")).await.unwrap(), Vec::<PathBuf>::new());
    }

    // Test sending a local file to the remote location
    {
        let bytes = gen_random_bytes(rng, 100..1000);
        let file_name_local: PathBuf = gen_random_string(rng, 10..20).into();
        let file_name_remote: PathBuf = gen_random_string(rng, 10..20).into();

        let temp_dir = tempfile::TempDir::new().unwrap();
        let local_path = temp_dir.path().join(file_name_local);
        std::fs::write(&local_path, &bytes).unwrap();
        fs.put(&local_path, &file_name_remote).await.unwrap();
        assert_eq!(
            fs.ls(Path::new(".")).await.unwrap(),
            [file_name_remote.clone()]
        );

        assert!(fs.file_exists(&file_name_remote).await.unwrap());
        assert_eq!(
            fs.ls(Path::new(".")).await.unwrap(),
            [file_name_remote.clone()]
        );
        fs.del_file(&file_name_remote).await.unwrap();
        assert!(!fs.file_exists(&file_name_remote).await.unwrap());
        assert_eq!(fs.ls(Path::new(".")).await.unwrap(), Vec::<PathBuf>::new());
    }

    // Test creating a deep dir and that it exists
    {
        let deep_dir = (0..10)
            .into_iter()
            .map(|_| gen_random_string(rng, 10..20))
            .fold(PathBuf::new(), |so_far, curr| {
                so_far.join(PathBuf::from(curr))
            });

        assert!(!fs.dir_exists(&deep_dir).await.unwrap());
        fs.mkdir_p(&deep_dir).await.unwrap();
        assert!(fs.dir_exists(&deep_dir).await.unwrap());
    }
}

#[tokio::test]
#[rstest]
async fn virtual_filesystem(random_seed: Seed) {
    println!("Starting test for in-memory filesystem...");
    let mut rng = make_seedable_rng(random_seed);

    let fs = make_inmemory_filesystem();
    test_store(fs.as_ref(), &mut rng).await;

    println!("End of test for in-memory filesystem reached.");
}

#[tokio::test]
#[rstest]
async fn local_filesystem(random_seed: Seed) {
    println!("Starting test for local filesystem...");
    let mut rng = make_seedable_rng(random_seed);

    let temp_dir = tempfile::TempDir::new().unwrap();

    let fs = make_store(&Arc::new(PathDescriptor::Local(temp_dir.path().to_owned()))).unwrap();
    test_store(fs.as_ref(), &mut rng).await;

    println!("End of test for local filesystem reached.");
}

#[ignore = "Prepare the server first then run this"]
#[tokio::test]
#[rstest]
async fn sftp_filesystem(_random_seed: Seed) {
    println!("Starting test for sftp filesystem...");

    // TODO: complete this test

    // let mut rng = make_seedable_rng(random_seed);

    // let fs = make_store(&Arc::new(PathDescriptor::Sftp {
    //     username,
    //     remote_address,
    //     remote_path,
    //     identity,
    // }))
    // .unwrap();

    // test_store(fs.as_ref(), &mut rng).await;

    println!("End of test for sftp filesystem reached.");
}
