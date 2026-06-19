use crate::{
    make_inmemory_filesystem, make_store, path_descriptor::PathDescriptor, traits::StoreDestination,
};
use logging::init_logging;
use rstest::rstest;
use russh::keys::ssh_encoding::EncodePem;
use std::{
    fmt::{Debug, Display},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};
use test_utils::random::{
    Rng, Seed, gen_random_bytes, gen_random_string, make_seedable_rng, random_seed,
};
use utils::podman::Podman;

async fn test_store<E: Display + Debug, S: StoreDestination<Error = E> + ?Sized>(
    fs: &S,
    rng: &mut impl Rng,
) {
    assert!(fs.ls(Path::new(".")).await.unwrap().is_empty());

    // Test that random files and directory names don't exist
    for _ in 0..10 {
        let name = gen_random_string(rng, 10..=50);
        assert!(!fs.file_exists(Path::new(&name)).await.unwrap());
        assert!(!fs.dir_exists(Path::new(&name)).await.unwrap());
    }

    // Test writing file from memory
    {
        let bytes = gen_random_bytes(rng, 100..1000);
        let file_name: PathBuf = gen_random_string(rng, 10..20).into();

        fs.put_from_memory(&bytes, &file_name).await.unwrap();

        let bytes_read = fs.get_to_memory(&file_name).await.unwrap();
        assert_eq!(bytes_read, bytes);

        assert!(fs.file_exists(&file_name).await.unwrap());
        assert_eq!(
            fs.ls(Path::new(".")).await.unwrap(),
            std::slice::from_ref(&file_name)
        );
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
            std::slice::from_ref(&file_name_remote)
        );

        assert!(fs.file_exists(&file_name_remote).await.unwrap());
        assert_eq!(
            fs.ls(Path::new(".")).await.unwrap(),
            std::slice::from_ref(&file_name_remote)
        );
        fs.del_file(&file_name_remote).await.unwrap();
        assert!(!fs.file_exists(&file_name_remote).await.unwrap());
        assert_eq!(fs.ls(Path::new(".")).await.unwrap(), Vec::<PathBuf>::new());
    }

    // Test creating a deep dir and that it exists
    {
        let deep_dir = (0..10)
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
#[trace]
async fn virtual_filesystem(random_seed: Seed) {
    println!("Starting test for in-memory filesystem...");
    let mut rng = make_seedable_rng(random_seed);

    let fs = make_inmemory_filesystem();
    fs.init().await.unwrap();
    test_store(fs.as_ref(), &mut rng).await;

    println!("End of test for in-memory filesystem reached.");
}

#[tokio::test]
#[rstest]
#[trace]
async fn local_filesystem(random_seed: Seed) {
    println!("Starting test for local filesystem...");
    let mut rng = make_seedable_rng(random_seed);

    let temp_dir = tempfile::TempDir::new().unwrap();

    let fs = make_store(&Arc::new(PathDescriptor::Local(temp_dir.path().to_owned()))).unwrap();
    fs.init().await.unwrap();
    test_store(fs.as_ref(), &mut rng).await;

    println!("End of test for local filesystem reached.");
}

#[tokio::test]
#[rstest]
#[trace]
async fn sftp_filesystem(
    random_seed: Seed,
    #[values(
        // Test without any qualifier
        "test-dir/", "test-dir", "test-dir/abc", "test-dir/abc/",
        // Tests with ./
         "./test-dir/", "./test-dir", "./test-dir/abc", "./test-dir/abc/",
        // Note: /config/ is the root dir in the ssh server we're using in this test
        "/config/test-dir/", "/config/test-dir", "/config/test-dir/abc/", "/config/test-dir/abc",
    )]
    base_remote_path: String,
) {
    init_logging();

    // Podman is needed to make this work, so we guard it behind an env var
    if std::env::var("SNAPSYNC_CONTAINERIZED_TESTS").is_err() {
        eprintln!("Warning: Skipping sftp containerized tests");
        return;
    }

    println!("Starting test for sftp filesystem...");

    let username = "some_user";

    let priv_key = gen_ssh_private_key().unwrap();
    let public_key = priv_key.public_key().clone();

    let priv_key_openssh_format_str = priv_key
        .encode_pem_string(russh::keys::ssh_key::LineEnding::LF)
        .unwrap();

    let mut rng = make_seedable_rng(random_seed);

    // Container: https://docs.linuxserver.io/images/docker-openssh-server
    // Note: To access with password for debugging, add the following args (-e for env var)
    // `-e PASSWORD_ACCESS=true -e USER_PASSWORD=YourPassword`
    // To ssh:
    // ssh -o IdentitiesOnly=yes -o PreferredAuthentications=password -p <port>  some_user@localhost
    // You need IdentityOnly in case you get the error "Too many authentication failures"
    let mut podman = Podman::new("SftpTest", "lscr.io/linuxserver/openssh-server:latest")
        .with_port_mapping(None, 2222)
        .with_env("USER_NAME", username)
        .with_env("PUID", "1000")
        .with_env("PGID", "1000")
        .with_env("TZ", "Etc/UTC")
        .with_env("PUBLIC_KEY", &public_key.to_openssh().unwrap());

    podman.run();

    let ssh_port = podman.get_port_mapping(2222).unwrap();

    let descriptor = Arc::new(PathDescriptor::Sftp {
        username: username.to_string(),
        remote_address: format!("127.0.0.1:{ssh_port}"),
        remote_path: base_remote_path,
        identity: crate::path_descriptor::StringFileData::InMemory(priv_key_openssh_format_str),
    });

    // - The container's SSH server takes an unpredictable time to generate host
    //   keys and start listening.
    // - A fixed sleep races that startup and intermittently hits a half-ready server.
    // - So retry building and initializing the store until it succeeds or a deadline passes.
    // - Each failed attempt returns quickly now because the store bounds its connection.
    let fs = connect_with_retry(&descriptor).await;

    test_store(fs.as_ref(), &mut rng).await;

    println!("End of test for sftp filesystem reached.");
}

async fn connect_with_retry(
    descriptor: &Arc<PathDescriptor>,
) -> Arc<dyn StoreDestination<Error = anyhow::Error>> {
    const MAX_ATTEMPTS: u32 = 30;
    const RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

    let mut last_error = String::new();
    for attempt in 1..=MAX_ATTEMPTS {
        match make_store(descriptor) {
            Ok(fs) => match fs.init().await {
                Ok(()) => return fs,
                Err(e) => last_error = format!("init failed: {e}"),
            },
            Err(e) => last_error = format!("store creation failed: {e}"),
        }

        eprintln!("SFTP server not ready (attempt {attempt}/{MAX_ATTEMPTS}): {last_error}");
        tokio::time::sleep(RETRY_INTERVAL).await;
    }

    panic!(
        "SFTP server never became ready after {MAX_ATTEMPTS} attempts. Last error: {last_error}"
    );
}

fn gen_ssh_private_key() -> anyhow::Result<russh::keys::PrivateKey> {
    let key = russh::keys::PrivateKey::random(
        &mut rand_core::OsRng,
        russh::keys::ssh_key::Algorithm::Ed25519,
    )?;
    Ok(key)
}

#[rstest]
#[case(true)] // happy path: matching credentials
#[case(false)] // negative path: wrong credentials -> auth failure
#[tokio::test]
#[trace]
async fn s3_filesystem(#[case] creds_ok: bool, random_seed: Seed) {
    init_logging();

    if std::env::var("SNAPSYNC_CONTAINERIZED_TESTS").is_err() {
        eprintln!(
            "Skipping S3 test; set SNAPSYNC_CONTAINERIZED_TESTS=1 to run containerized tests."
        );
        return;
    }

    let mut rng = make_seedable_rng(random_seed);
    let bucket = format!("test-{}", gen_random_string(&mut rng, 8..12).to_lowercase());
    let base_path: PathBuf = PathBuf::from(gen_random_string(&mut rng, 6..10));

    // ---- Start MinIO (real auth enforced) -----------------------------------
    // API on 9000, Console on 9001
    let access_key = "minio_test_user";
    let secret_key = "minio_test_secret_12345678";

    // Bitnami image auto-starts the server with these env vars; no command needed.
    let mut podman = Podman::new(
        "S3Test",
        "docker.io/bitnamilegacy/minio:2025.7.23-debian-12-r5",
    )
    .with_port_mapping(None, 9000)
    .with_port_mapping(None, 9001)
    .with_env("MINIO_ROOT_USER", access_key)
    .with_env("MINIO_ROOT_PASSWORD", secret_key);
    podman.run();

    let api_port = podman.get_port_mapping(9000).expect("minio api port");
    // let console_port = podman.get_port_mapping(9001).unwrap(); // not used

    // Give MinIO time to come up
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{api_port}");

    // ---- Credentials INI (good vs bad) --------------------------------------
    let (ini_ak, ini_sk) = if creds_ok {
        (access_key, secret_key)
    } else {
        ("wrong", "wrong")
    };

    let ini =
        format!("[default]\naws_access_key_id = {ini_ak}\naws_secret_access_key = {ini_sk}\n");
    let creds_file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(creds_file.path(), ini).unwrap();

    // Distinct bucket to avoid cross-case interference
    let test_bucket = if creds_ok {
        bucket.clone()
    } else {
        format!("{bucket}-bad")
    };

    let desc = PathDescriptor::from_str(&format!(
        "s3:bucket={test_bucket};s3-path={};region=us-east-1;endpoint={endpoint};credentials-path={}",
        base_path.display(),
        creds_file.path().display(),
    ))
    .unwrap();

    let fs = make_store(&Arc::new(desc)).unwrap();

    if creds_ok {
        fs.init().await.unwrap(); // should create bucket + base prefix
        test_store(fs.as_ref(), &mut rng).await;
    } else {
        // Init should attempt a bucket op and fail with auth error.
        assert!(
            fs.init().await.is_err(),
            "expected auth failure against MinIO with wrong credentials"
        );
    }
}
