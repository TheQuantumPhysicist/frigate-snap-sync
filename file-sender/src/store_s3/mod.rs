use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::{
    Client,
    config::{Region, SharedCredentialsProvider},
    primitives::ByteStream,
};
use bytes::Bytes;
use tokio::sync::OnceCell;

use crate::{
    path_descriptor::{PathDescriptor, StringFileData},
    traits::StoreDestination,
};

const S3_KEY_CREDENTIALS_PROFILE_DEFAULT: &str = "default"; // default profile if not specified

pub struct AsyncS3Impl {
    client: OnceCell<Client>,
    bucket: String,
    base_prefix: String, // "" or ends with '/'
    region: Option<String>,
    endpoint: Option<String>,
    credentials_path: StringFileData,
    credentials_profile: Option<String>,
    path_descriptor: Arc<PathDescriptor>,
}

impl AsyncS3Impl {
    pub fn new(
        bucket: impl Into<String>,
        base_path: impl AsRef<Path>,
        region: Option<String>,
        endpoint: Option<String>,
        credentials_path: StringFileData,
        credentials_profile: Option<String>,
        pd: Arc<PathDescriptor>,
    ) -> Self {
        let mut base_prefix = base_path.as_ref().to_string_lossy().replace('\\', "/");
        if !base_prefix.is_empty() && !base_prefix.ends_with('/') {
            base_prefix.push('/');
        }

        Self {
            client: OnceCell::new(),
            bucket: bucket.into(),
            base_prefix,
            region,
            endpoint,
            credentials_path,
            credentials_profile,
            path_descriptor: pd,
        }
    }

    async fn client(&self) -> Result<&Client> {
        self.client
            .get_or_try_init(|| async {
                // Base AWS config (optionally pin region)
                let mut defaults = aws_config::defaults(BehaviorVersion::latest());
                if let Some(r) = &self.region {
                    defaults = defaults.region(Region::new(r.clone()));
                }
                let sdk_cfg = defaults.load().await;

                // Read INI creds (profile defaults to "default")
                let contents = self.credentials_path.clone().into_file_data()?;
                let profile = self
                    .credentials_profile
                    .as_deref()
                    .unwrap_or(S3_KEY_CREDENTIALS_PROFILE_DEFAULT);
                let (akid, secret, token) = parse_aws_credentials_ini(&contents, profile)?;

                let provider = SharedCredentialsProvider::new(Credentials::new(
                    akid,
                    secret,
                    token,
                    None,
                    "ini-profile",
                ));

                let mut builder =
                    aws_sdk_s3::config::Builder::from(&sdk_cfg).credentials_provider(provider);

                if let Some(ep) = &self.endpoint {
                    // Generic S3 (MinIO/Ceph/LocalStack, etc.)
                    builder = builder.endpoint_url(ep).force_path_style(true);
                }

                Ok::<Client, anyhow::Error>(Client::from_conf(builder.build()))
            })
            .await
            .context("S3 client init failed")
    }

    #[inline]
    #[allow(clippy::match_same_arms)]
    fn key(&self, rel: &Path) -> String {
        // Build a clean "relative key fragment" without "." segments
        let frag = rel
            .components()
            .filter_map(|c| match c {
                Component::CurDir => None, // skip "."
                Component::Normal(s) => Some(s.to_string_lossy()),
                // Ignore any other components (RootDir, ParentDir, Prefix) to keep behavior sane
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");

        if frag.is_empty() {
            // "." (or empty) → just base prefix
            self.base_prefix.clone()
        } else {
            // base_prefix (may already end with '/') + frag
            if self.base_prefix.is_empty() {
                frag
            } else if self.base_prefix.ends_with('/') {
                format!("{}{}", self.base_prefix, frag)
            } else {
                format!("{}/{}", self.base_prefix, frag)
            }
        }
    }

    #[inline]
    fn dir_key(&self, rel: &Path) -> String {
        // Ensure a trailing '/' for directory prefixes
        let mut k = self.key(rel);
        if !k.is_empty() && !k.ends_with('/') {
            k.push('/');
        }
        k
    }
}

fn parse_aws_credentials_ini(
    contents: &str,
    profile: &str,
) -> Result<(String, String, Option<String>)> {
    let map = ini::inistr!(contents);

    let sec = map
        .get(profile)
        .ok_or_else(|| anyhow!("profile [{profile}] not found in credentials file"))?;

    let akid = sec
        .get("aws_access_key_id")
        .and_then(Clone::clone)
        .map(|s| s.trim().to_string())
        .ok_or_else(|| anyhow!("missing aws_access_key_id in [{profile}]"))?;

    let secret = sec
        .get("aws_secret_access_key")
        .and_then(Clone::clone)
        .map(|s| s.trim().to_string())
        .ok_or_else(|| anyhow!("missing aws_secret_access_key in [{profile}]"))?;

    let token = sec
        .get("aws_session_token")
        .and_then(Clone::clone)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Ok((akid, secret, token))
}

#[async_trait]
impl StoreDestination for AsyncS3Impl {
    type Error = anyhow::Error;

    async fn init(&self) -> Result<()> {
        let c = self.client().await.context("S3 client init failed")?;

        // Does the bucket exist?
        let bucket_exists = c.head_bucket().bucket(&self.bucket).send().await.is_ok();

        if !bucket_exists {
            let mut req = c.create_bucket().bucket(&self.bucket);

            // Only include a location constraint if the user explicitly provided a region.
            // If the server requires one and it's missing, let the server return an error.
            if let Some(r) = self.region.as_deref() {
                use aws_sdk_s3::types::CreateBucketConfiguration;
                req = req.create_bucket_configuration(
                    CreateBucketConfiguration::builder()
                        .location_constraint(r.into())
                        .build(),
                );
            }

            req.send().await.with_context(|| {
                format!(
                    "create_bucket failed (bucket='{}', region={:?}, endpoint={:?})",
                    self.bucket, self.region, self.endpoint
                )
            })?;
        }

        // Ensure base prefix "dir" marker
        let base = self.dir_key(std::path::Path::new(""));
        if !base.is_empty() {
            c.put_object()
                .bucket(&self.bucket)
                .key(&base)
                .body(aws_sdk_s3::primitives::ByteStream::from_static(b""))
                .send()
                .await
                .with_context(|| {
                    format!(
                        "put_object(dir marker) failed: bucket='{}' key='{}'",
                        self.bucket, base
                    )
                })?;
        }

        Ok(())
    }

    async fn ls(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let c = self.client().await?;
        let prefix = self.dir_key(path);

        let resp = c
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&prefix)
            .delimiter("/")
            .send()
            .await?;

        let mut out = Vec::new();

        // Files directly under the prefix
        for o in resp.contents() {
            if let Some(k) = o.key()
                && let Some(leaf) = k.strip_prefix(&prefix)
                && !leaf.is_empty()
                && !leaf.contains('/')
            {
                out.push(PathBuf::from(leaf));
            }
        }

        // Immediate subdirectories
        for p in resp.common_prefixes() {
            if let Some(k) = p.prefix()
                && let Some(mut leaf) = k.strip_prefix(&prefix)
            {
                // prefixes returned by S3 end with '/', remove it
                leaf = leaf.strip_suffix('/').unwrap_or(leaf);
                if !leaf.is_empty() {
                    out.push(PathBuf::from(leaf));
                }
            }
        }

        Ok(out)
    }

    async fn del_file(&self, path: &Path) -> Result<()> {
        let c = self.client().await?;
        c.delete_object()
            .bucket(&self.bucket)
            .key(self.key(path))
            .send()
            .await?;
        Ok(())
    }

    async fn mkdir_p(&self, path: &Path) -> Result<()> {
        let c = self.client().await?;
        c.put_object()
            .bucket(&self.bucket)
            .key(self.dir_key(path))
            .body(ByteStream::from_static(b""))
            .send()
            .await?;
        Ok(())
    }

    async fn put(&self, from: &Path, to: &Path) -> Result<()> {
        let c = self.client().await?;
        let mut f = tokio::fs::File::open(from)
            .await
            .with_context(|| format!("open local file {}", from.display()))?;
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut f, &mut buf).await?;
        c.put_object()
            .bucket(&self.bucket)
            .key(self.key(to))
            .body(ByteStream::from(Bytes::from(buf)))
            .send()
            .await?;
        Ok(())
    }

    async fn put_from_memory(&self, from: &[u8], to: &Path) -> Result<()> {
        let c = self.client().await?;
        c.put_object()
            .bucket(&self.bucket)
            .key(self.key(to))
            .body(ByteStream::from(Bytes::copy_from_slice(from)))
            .send()
            .await?;
        Ok(())
    }

    async fn get_to_memory(&self, from: &Path) -> Result<Vec<u8>> {
        let c = self.client().await?;
        let obj = c
            .get_object()
            .bucket(&self.bucket)
            .key(self.key(from))
            .send()
            .await?;
        let bytes = obj.body.collect().await?.into_bytes();
        Ok(bytes.to_vec())
    }

    async fn dir_exists(&self, path: &Path) -> Result<bool> {
        let c = self.client().await?;
        let resp = c
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(self.dir_key(path))
            .max_keys(1)
            .send()
            .await?;
        Ok(resp.key_count().unwrap_or(0) > 0)
    }

    async fn file_exists(&self, path: &Path) -> Result<bool> {
        let c = self.client().await?;
        Ok(c.head_object()
            .bucket(&self.bucket)
            .key(self.key(path))
            .send()
            .await
            .is_ok())
    }

    fn path_descriptor(&self) -> &Arc<PathDescriptor> {
        &self.path_descriptor
    }
}
