pub mod config;
pub mod helpers;
pub mod json;
pub mod traits;

use crate::json::stats::{Stats, StatsProps};
use anyhow::Context;
use async_trait::async_trait;
use config::{FrigateApiAuthConfig, FrigateApiConfig};
use json::review::Review;
use reqwest::{Method, Response, StatusCode};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tracing::trace_span;
use traits::FrigateApi;

pub fn make_frigate_client(config: FrigateApiConfig) -> anyhow::Result<Arc<dyn FrigateApi>> {
    let span = trace_span!("make_frigate_client");
    let _enter = span.enter();

    tracing::trace!("Begin make_frigate_client function");
    // cookie_store holds Frigate's session cookie after login, so authenticated
    // requests carry it automatically without any hand-maintained session state.
    let builder = reqwest::ClientBuilder::new().cookie_store(true);

    tracing::trace!("Builder created");

    let client = match &config.frigate_api_proxy {
        Some(proxy) => builder
            .proxy(reqwest::Proxy::all(proxy).context("Invalid proxy URL")?)
            .build()
            .context("Building Frigate API with proxy")?,
        None => builder
            .build()
            .context("Building Frigate API without proxy")?,
    };

    tracing::trace!("Building client done");

    let result = FrigateApiClient { client, config };

    tracing::trace!("Returning API object");

    Ok(Arc::new(result))
}

struct FrigateApiClient {
    client: reqwest::Client,
    config: FrigateApiConfig,
}

// Body of Frigate's login endpoint. Frigate names the user field `user`.
#[derive(Serialize)]
struct LoginRequest<'a> {
    user: &'a str,
    password: &'a str,
}

impl FrigateApiClient {
    // - Sends a GET and transparently handles Frigate's session authentication.
    // - A non-401 response is returned as-is (after the usual status check).
    // - A 401 with credentials configured triggers one login and one retry.
    // - A 401 with no credentials is surfaced as an error, not silently retried.
    // - There is no auth-status flag: the cookie store is the only session state,
    //   and a fresh 401 is the only trigger, so an expired token self-heals.
    async fn send_authenticated_request(
        &self,
        method: Method,
        url: &str,
    ) -> anyhow::Result<Response> {
        let response = self.send_request(method.clone(), url).await?;

        if response.status() != StatusCode::UNAUTHORIZED {
            return response
                .error_for_status()
                .context("Frigate API request failed");
        }

        let Some(auth) = &self.config.frigate_api_auth else {
            return response.error_for_status().context(
                "Frigate API returned 401 but no credentials are configured for authentication",
            );
        };

        tracing::debug!("Frigate API returned 401; logging in and retrying the request once.");
        self.login(auth).await?;

        self.send_request(method, url)
            .await?
            .error_for_status()
            .context("Frigate API request failed after re-authenticating")
    }

    async fn send_request(&self, method: Method, url: &str) -> anyhow::Result<Response> {
        self.client
            .request(method, url)
            .headers(json_headers_map())
            .send()
            .await
            .context("Sending Frigate API request failed")
    }

    async fn login(&self, auth: &FrigateApiAuthConfig) -> anyhow::Result<()> {
        let base_url = &self.config.frigate_api_base_url;
        let url = format!("{base_url}/api/login");

        tracing::debug!("Logging into the Frigate API.");
        // Frigate's login takes a JSON body of {user, password} and returns the
        // session as a Set-Cookie JWT; it uses no CSRF token or extra headers.
        let response = self
            .client
            .post(&url)
            .headers(json_headers_map())
            .json(&LoginRequest {
                user: &auth.username,
                password: &auth.password,
            })
            .send()
            .await
            .context("Sending Frigate login request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|e| format!("<failed to read login error body: {e}>"));
            return Err(anyhow::anyhow!(
                "Frigate login failed at {url} with status {status}: {body}"
            ));
        }

        tracing::debug!("Frigate API login succeeded.");

        Ok(())
    }
}

#[async_trait]
impl FrigateApi for FrigateApiClient {
    async fn test_call(&self) -> anyhow::Result<()> {
        let base_url = &self.config.frigate_api_base_url;
        let url = format!("{base_url}/api/review/summary");

        let response = self.send_authenticated_request(Method::GET, &url).await?;
        let response_json = response.json::<Value>().await.context("Parsing response")?;

        // Review summaries always contain the key "last24Hours"
        match response_json.get("last24Hours") {
            Some(_) => {
                tracing::debug!("API test call succeeded with output: {response_json}");
            }
            None => {
                return Err(anyhow::anyhow!(
                    "Test request succeeded, but the response does not seem valid. Perhaps the URL is invalid: {response_json}"
                ));
            }
        }

        Ok(())
    }

    async fn review(&self, id: &str) -> anyhow::Result<Review> {
        let base_url = &self.config.frigate_api_base_url;
        let url = format!("{base_url}/api/review/{id}");
        let response = self.send_authenticated_request(Method::GET, &url).await?;
        let result = response.json::<Review>().await?;

        tracing::debug!("Call `review` with id {id} with response: {:?}", result);

        Ok(result)
    }

    async fn stats(&self) -> anyhow::Result<Box<dyn StatsProps>> {
        let base_url = &self.config.frigate_api_base_url;
        let url = format!("{base_url}/api/stats");
        let response = self.send_authenticated_request(Method::GET, &url).await?;
        let result = response.json::<Stats>().await?;

        tracing::debug!("Call `stats` with response: {:?}", result);

        Ok(Box::new(result))
    }

    async fn recording_clip(
        &self,
        camera_label: &str,
        start_ts: f64,
        end_ts: f64,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let base_url = &self.config.frigate_api_base_url;
        let url = format!("{base_url}/api/{camera_label}/start/{start_ts}/end/{end_ts}/clip.mp4");
        let response = self.send_authenticated_request(Method::GET, &url).await?;
        let result = response.bytes().await?;

        if !is_valid_mp4(&result) {
            return Err(anyhow::anyhow!(
                "The file returned in `recording_clip` API call is not a valid MP4 file. Parameters: [start,end] times [{start_ts},{end_ts}]"
            ));
        }

        if result.is_empty() {
            return Ok(None);
        }

        // Format timestamps with 6 digits of decimals
        let start_ts = format!("{start_ts:.6}");
        let end_ts = format!("{end_ts:.6}");

        tracing::debug!(
            "Call `recording_clip` with [start,end] times [{start_ts},{end_ts}] with response of size: {} bytes",
            result.len()
        );

        Ok(Some(result.into()))
    }
}

fn json_headers_map() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Accept",
        "application/json".parse().expect("Parsing must work"),
    );
    headers
}

/// Basic check that the file provided is an MP4 file
fn is_valid_mp4(data: &[u8]) -> bool {
    data.len() > 11 && &data[4..8] == b"ftyp"
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::{fixture, rstest};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use test_utils::random::{Seed, gen_random_bytes, make_seedable_rng, random_seed};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // Minimal but structurally valid /api/stats body so the response deserializes.
    const MINIMAL_STATS_JSON: &str = r#"{
        "cameras": {},
        "detectors": {},
        "detection_fps": 0.0,
        "service": { "uptime": 123456, "version": "0.14.1", "last_updated": 1700000000 },
        "processes": {}
    }"#;

    fn client_for(base_url: String, auth: Option<FrigateApiAuthConfig>) -> Arc<dyn FrigateApi> {
        make_frigate_client(FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
            frigate_api_auth: auth,
            delay_after_startup: std::time::Duration::ZERO,
        })
        .unwrap()
    }

    struct MockFrigate {
        base_url: String,
        login_calls: Arc<AtomicUsize>,
    }

    // A tiny stand-in for Frigate's HTTP API, just enough to drive the auth flow.
    // - GET /api/stats: 200 with stats when `require_auth` is off, or when a session
    //   cookie is present; otherwise 401.
    // - POST /api/login: 200 with a Set-Cookie session when `login_succeeds`, else 401;
    //   it also counts how many times login was called.
    // Each connection is answered once and closed, so the reqwest cookie store captures
    // the Set-Cookie and resends it on the retried request.
    async fn start_mock_frigate(require_auth: bool, login_succeeds: bool) -> MockFrigate {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let login_calls = Arc::new(AtomicUsize::new(0));
        let login_calls_inner = login_calls.clone();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let login_calls = login_calls_inner.clone();
                tokio::spawn(async move {
                    // Read only the request head, up to the blank line.
                    let mut data = Vec::new();
                    let mut chunk = [0u8; 1024];
                    loop {
                        let Ok(read) = socket.read(&mut chunk).await else {
                            return;
                        };
                        if read == 0 {
                            break;
                        }
                        data.extend_from_slice(&chunk[..read]);
                        if data.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }

                    let request = String::from_utf8_lossy(&data);
                    let first_line = request.lines().next().unwrap_or("");
                    let has_cookie = request
                        .lines()
                        .any(|line| line.to_ascii_lowercase().starts_with("cookie:"));

                    let (status, set_cookie, body): (&str, &str, &str) =
                        if first_line.starts_with("POST") && first_line.contains("/api/login") {
                            login_calls.fetch_add(1, Ordering::SeqCst);
                            if login_succeeds {
                                (
                                    "200 OK",
                                    "Set-Cookie: frigate_token=test-jwt; Path=/\r\n",
                                    "",
                                )
                            } else {
                                ("401 Unauthorized", "", "login failed")
                            }
                        } else if first_line.contains("/api/stats") {
                            if !require_auth || has_cookie {
                                ("200 OK", "", MINIMAL_STATS_JSON)
                            } else {
                                ("401 Unauthorized", "", "")
                            }
                        } else {
                            ("404 Not Found", "", "")
                        };

                    let response = format!(
                        "HTTP/1.1 {status}\r\n{set_cookie}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        MockFrigate {
            base_url: format!("http://{addr}"),
            login_calls,
        }
    }

    #[fixture]
    pub fn base_url() -> String {
        "http://127.0.0.1:5000".to_string()
    }

    #[test]
    fn is_valid_mp4_fixed_cases() {
        // Empty body (a not-ready / zero-byte clip) must be rejected.
        assert!(!is_valid_mp4(b""));
        // "ftyp" present but shorter than the 12-byte minimum.
        assert!(!is_valid_mp4(b"\x00\x00\x00\x00ftyp"));
        assert!(!is_valid_mp4(b"\x00\x00\x00\x00ftyp123"));
        // Long enough but the wrong box type at bytes 4..8.
        assert!(!is_valid_mp4(b"\x00\x00\x00\x18moovisom\x00\x00\x00\x00"));
        // A minimal valid ftyp header.
        assert!(is_valid_mp4(b"\x00\x00\x00\x18ftypisom\x00\x00\x00\x00"));
    }

    #[rstest]
    #[trace]
    fn is_valid_mp4_randomized(random_seed: Seed) {
        let mut rng = make_seedable_rng(random_seed);

        // 4 leading bytes, the "ftyp" marker at bytes 4..8, then a non-empty tail:
        // length exceeds 11 and the marker matches, so it is accepted.
        let mut valid = gen_random_bytes(&mut rng, 4..5);
        valid.extend_from_slice(b"ftyp");
        valid.extend_from_slice(&gen_random_bytes(&mut rng, 4..64));
        assert!(is_valid_mp4(&valid));

        // Same length class but a different box type at bytes 4..8: rejected.
        let mut wrong_marker = gen_random_bytes(&mut rng, 4..5);
        wrong_marker.extend_from_slice(b"moov");
        wrong_marker.extend_from_slice(&gen_random_bytes(&mut rng, 0..64));
        assert!(!is_valid_mp4(&wrong_marker));

        // Anything 11 bytes or shorter is rejected regardless of contents.
        let too_short = gen_random_bytes(&mut rng, 0..12);
        assert!(!is_valid_mp4(&too_short));
    }

    #[test]
    fn deserialize_representative_stats() {
        // Mirrors the shape of Frigate's /api/stats response for the fields we model.
        let json = r#"{
            "cameras": {
                "front_door": {
                    "camera_fps": 5.0,
                    "process_fps": 5.0,
                    "skipped_fps": 0.0,
                    "detection_fps": 0.1,
                    "detection_enabled": true
                }
            },
            "detectors": { "cpu1": { "inference_speed": 8.5, "detection_start": 0.0 } },
            "detection_fps": 0.1,
            "service": { "uptime": 123456, "version": "0.14.1", "last_updated": 1700000000 },
            "processes": { "go2rtc": { "pid": 42 } }
        }"#;

        let stats: Stats =
            serde_json::from_str(json).expect("representative stats must deserialize");

        assert_eq!(
            stats.uptime_duration(),
            std::time::Duration::from_secs(123_456)
        );
        assert_eq!(
            StatsProps::uptime(&stats),
            std::time::Duration::from_secs(123_456)
        );
    }

    #[test]
    fn deserialize_representative_review() {
        // Mirrors the shape of Frigate's /api/review/{id} response.
        let json = r#"{
            "id": "1700000000.123-abcde",
            "camera": "front_door",
            "start_time": 1700000000.5,
            "end_time": 1700000010.5,
            "has_been_reviewed": false,
            "severity": "alert",
            "thumb_path": "/media/frigate/clips/review/thumb.webp",
            "data": {
                "detections": ["1700000000.123-abcde"],
                "objects": ["person"],
                "sub_labels": [],
                "zones": ["yard"],
                "audio": []
            }
        }"#;

        let review: Review =
            serde_json::from_str(json).expect("representative review must deserialize");

        assert_eq!(review.id, "1700000000.123-abcde");
        assert_eq!(review.camera, "front_door");
        // Exact float comparison via bit pattern to avoid a lint-flagged `==` on floats.
        assert_eq!(review.start_time.to_bits(), 1_700_000_000.5_f64.to_bits());
        assert_eq!(
            review.end_time.map(f64::to_bits),
            Some(1_700_000_010.5_f64.to_bits())
        );
        assert_eq!(review.data.objects, vec!["person".to_string()]);
        assert!(!review.has_been_reviewed);
    }

    // A 200 response means no login is attempted, even with credentials set:
    // authentication is reactive, driven only by a 401.
    #[tokio::test]
    async fn successful_request_does_not_log_in() {
        let server = start_mock_frigate(false, true).await;
        let client = client_for(
            server.base_url.clone(),
            Some(FrigateApiAuthConfig {
                username: "u".to_string(),
                password: "p".to_string(),
            }),
        );

        let stats = client.stats().await.unwrap();
        assert_eq!(stats.uptime(), std::time::Duration::from_secs(123_456));
        assert_eq!(server.login_calls.load(Ordering::SeqCst), 0);
    }

    // On a 401 with credentials, the client logs in once and retries the request,
    // and the retry carries the session cookie issued by the login response.
    #[tokio::test]
    async fn logs_in_after_401_then_retries_with_cookie() {
        let server = start_mock_frigate(true, true).await;
        let client = client_for(
            server.base_url.clone(),
            Some(FrigateApiAuthConfig {
                username: "u".to_string(),
                password: "p".to_string(),
            }),
        );

        let stats = client.stats().await.unwrap();
        assert_eq!(stats.uptime(), std::time::Duration::from_secs(123_456));
        assert_eq!(server.login_calls.load(Ordering::SeqCst), 1);
    }

    // A 401 with no configured credentials is surfaced as an error, never retried.
    #[tokio::test]
    async fn unauthorized_without_credentials_is_an_error() {
        let server = start_mock_frigate(true, true).await;
        let client = client_for(server.base_url.clone(), None);

        assert!(client.stats().await.is_err());
        assert_eq!(server.login_calls.load(Ordering::SeqCst), 0);
    }

    // A failed login (wrong credentials) surfaces as an error.
    #[tokio::test]
    async fn login_failure_surfaces_error() {
        let server = start_mock_frigate(true, false).await;
        let client = client_for(
            server.base_url.clone(),
            Some(FrigateApiAuthConfig {
                username: "u".to_string(),
                password: "wrong".to_string(),
            }),
        );

        assert!(client.stats().await.is_err());
    }

    #[tokio::test]
    #[rstest]
    #[trace]
    #[ignore = "If you want to run this, set the fixture url then run it"]
    async fn test_call(base_url: String) {
        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
            frigate_api_auth: None,
            delay_after_startup: std::time::Duration::ZERO,
        };
        let frigate_client = make_frigate_client(config).unwrap();
        frigate_client.test_call().await.unwrap();
    }

    #[tokio::test]
    #[rstest]
    #[trace]
    #[ignore = "If you want to run this, set the fixture url, set the parameters then run it"]
    async fn review(base_url: String) {
        let review_id = "1744534711.333822-vsz5s4";

        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
            frigate_api_auth: None,
            delay_after_startup: std::time::Duration::ZERO,
        };
        let frigate_client = make_frigate_client(config).unwrap();
        println!(
            "Review: {:?}",
            frigate_client.review(review_id).await.unwrap()
        );
    }

    #[tokio::test]
    #[rstest]
    #[trace]
    #[ignore = "If you want to run this, set the fixture url, set the parameters then run it"]
    async fn stats(base_url: String) {
        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
            frigate_api_auth: None,
            delay_after_startup: std::time::Duration::ZERO,
        };
        let frigate_client = make_frigate_client(config).unwrap();
        let stats = frigate_client.stats().await.unwrap();
        println!("Uptime: {:?}", stats.uptime());
    }

    #[tokio::test]
    #[rstest]
    #[trace]
    #[ignore = "If you want to run this, set the fixture url, set the parameters then run it"]
    async fn recording_clip(base_url: String) {
        let camera_label = "my_camera";
        let start_timestamp = 1_744_534_711.333_822;
        let end_timestamp = 1_744_534_731.134_57;

        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
            frigate_api_auth: None,
            delay_after_startup: std::time::Duration::ZERO,
        };
        let frigate_client = make_frigate_client(config).unwrap();
        let mov = frigate_client
            .recording_clip(camera_label, start_timestamp, end_timestamp)
            .await
            .unwrap()
            .unwrap();

        std::fs::write("test.mp4", mov).unwrap();
    }
}
