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
use tokio::sync::Mutex;
use tracing::trace_span;
use traits::FrigateApi;

pub fn make_frigate_client(config: FrigateApiConfig) -> anyhow::Result<Arc<dyn FrigateApi>> {
    let span = trace_span!("make_frigate_client");
    let _enter = span.enter();

    tracing::trace!("Begin make_frigate_client function");
    let mut builder = reqwest::ClientBuilder::new();
    if config.frigate_api_auth.is_some() {
        tracing::debug!("Frigate API authentication is configured");
        builder = builder.cookie_store(true);
    }

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

    let result = FrigateApiClient {
        client,
        config,
        logged_in: Mutex::new(false),
    };

    tracing::trace!("Returning API object");

    Ok(Arc::new(result))
}

struct FrigateApiClient {
    client: reqwest::Client,
    config: FrigateApiConfig,
    logged_in: Mutex<bool>,
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    user: &'a str,
    password: &'a str,
}

#[async_trait]
impl FrigateApi for FrigateApiClient {
    async fn test_call(&self) -> anyhow::Result<()> {
        let span = tracing::trace_span!("Frigate API test_call");
        let _enter = span.enter();
        tracing::trace!("Start");

        let base_url = &self.config.frigate_api_base_url;
        let url = format!("{base_url}/api/review/summary");

        tracing::trace!("Creating request");

        tracing::trace!("Submitting request to URL: {url}");
        let response = self
            .send_authenticated_request(Method::GET, &url)
            .await
            .context("Sending test request failed")?;

        tracing::trace!("Parsing response request");
        let response_json = response.json::<Value>().await.context("Parsing response")?;

        tracing::trace!("Printing results");
        // Review summaries always contain the key "last24Hours"
        match response_json.get("last24Hours") {
            Some(_) => {
                tracing::debug!("API test call succeeded with output: {response_json}",);
            }
            None => {
                return Err(anyhow::anyhow!(
                    "Test request succeeded, but the response does not seem valid. Perhaps the URL is invalid: {response_json}"
                ));
            }
        }

        tracing::trace!("End");

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

impl FrigateApiClient {
    async fn send_authenticated_request(
        &self,
        method: Method,
        url: &str,
    ) -> anyhow::Result<Response> {
        if self.config.frigate_api_auth.is_some() {
            self.ensure_logged_in().await?;
        }

        let response = self.send_request(method.clone(), url).await?;
        if response.status() != StatusCode::UNAUTHORIZED || self.config.frigate_api_auth.is_none() {
            return response.error_for_status().context("Frigate API request failed");
        }

        tracing::debug!("Frigate API returned 401; logging in again and retrying request");
        {
            let mut logged_in = self.logged_in.lock().await;
            *logged_in = false;
        }
        self.ensure_logged_in().await?;

        self.send_request(method, url)
            .await?
            .error_for_status()
            .context("Frigate API request failed after login retry")
    }

    async fn send_request(&self, method: Method, url: &str) -> anyhow::Result<Response> {
        self.client
            .request(method, url)
            .headers(json_headers_map())
            .send()
            .await
            .context("Sending Frigate API request failed")
    }

    async fn ensure_logged_in(&self) -> anyhow::Result<()> {
        let Some(auth) = &self.config.frigate_api_auth else {
            return Ok(());
        };

        let mut logged_in = self.logged_in.lock().await;
        if *logged_in {
            return Ok(());
        }

        tracing::debug!("Logging into Frigate API");
        self.login(auth).await?;
        *logged_in = true;
        Ok(())
    }

    async fn login(&self, auth: &FrigateApiAuthConfig) -> anyhow::Result<()> {
        let base_url = &self.config.frigate_api_base_url;
        let url = format!("{base_url}/api/login");

        let response = self
            .client
            .post(&url)
            .header("Accept", "application/json")
            .header("X-CSRF-TOKEN", "1")
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
                .unwrap_or_else(|e| format!("Failed to read Frigate login error body: {e}"));
            return Err(anyhow::anyhow!(
                "Frigate login failed at {url} with status {status}: {body}"
            ));
        }

        tracing::debug!("Frigate API login succeeded");

        Ok(())
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    use rstest::{fixture, rstest};

    #[fixture]
    pub fn base_url() -> String {
        "http://127.0.0.1:5000".to_string()
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

    #[tokio::test]
    async fn test_call_without_auth_does_not_login() {
        let login_count = Arc::new(AtomicUsize::new(0));
        let summary_count = Arc::new(AtomicUsize::new(0));
        let base_url = spawn_test_server({
            let login_count = login_count.clone();
            let summary_count = summary_count.clone();
            move |request| {
                if request.starts_with("POST /api/login ") {
                    login_count.fetch_add(1, Ordering::SeqCst);
                    return response(500, "unexpected login");
                }

                if request.starts_with("GET /api/review/summary ") {
                    summary_count.fetch_add(1, Ordering::SeqCst);
                    return json_response(r#"{"last24Hours":[]}"#);
                }

                response(404, "not found")
            }
        })
        .await;

        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
            frigate_api_auth: None,
            delay_after_startup: std::time::Duration::ZERO,
        };
        let frigate_client = make_frigate_client(config).unwrap();

        frigate_client.test_call().await.unwrap();

        assert_eq!(login_count.load(Ordering::SeqCst), 0);
        assert_eq!(summary_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_call_with_auth_logs_in_and_sends_cookie() {
        let login_count = Arc::new(AtomicUsize::new(0));
        let summary_count = Arc::new(AtomicUsize::new(0));
        let base_url = spawn_test_server({
            let login_count = login_count.clone();
            let summary_count = summary_count.clone();
            move |request| {
                if request.starts_with("POST /api/login ") {
                    login_count.fetch_add(1, Ordering::SeqCst);
                    assert!(request.contains(r#""user":"snap-sync""#));
                    assert!(request.contains(r#""password":"secret-password""#));
                    assert!(request.to_ascii_lowercase().contains("x-csrf-token: 1"));
                    return login_response();
                }

                if request.starts_with("GET /api/review/summary ") {
                    summary_count.fetch_add(1, Ordering::SeqCst);
                    assert!(request
                        .to_ascii_lowercase()
                        .contains("cookie: frigate_token=token"));
                    return json_response(r#"{"last24Hours":[]}"#);
                }

                response(404, "not found")
            }
        })
        .await;

        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
            frigate_api_auth: Some(FrigateApiAuthConfig {
                username: "snap-sync".to_string(),
                password: "secret-password".to_string(),
            }),
            delay_after_startup: std::time::Duration::ZERO,
        };
        let frigate_client = make_frigate_client(config).unwrap();

        frigate_client.test_call().await.unwrap();

        assert_eq!(login_count.load(Ordering::SeqCst), 1);
        assert_eq!(summary_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_call_with_auth_retries_login_after_unauthorized_response() {
        let login_count = Arc::new(AtomicUsize::new(0));
        let summary_count = Arc::new(AtomicUsize::new(0));
        let base_url = spawn_test_server({
            let login_count = login_count.clone();
            let summary_count = summary_count.clone();
            move |request| {
                if request.starts_with("POST /api/login ") {
                    login_count.fetch_add(1, Ordering::SeqCst);
                    return login_response();
                }

                if request.starts_with("GET /api/review/summary ") {
                    let count = summary_count.fetch_add(1, Ordering::SeqCst);
                    assert!(request
                        .to_ascii_lowercase()
                        .contains("cookie: frigate_token=token"));
                    if count == 0 {
                        return response(401, "unauthorized");
                    }
                    return json_response(r#"{"last24Hours":[]}"#);
                }

                response(404, "not found")
            }
        })
        .await;

        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
            frigate_api_auth: Some(FrigateApiAuthConfig {
                username: "snap-sync".to_string(),
                password: "secret-password".to_string(),
            }),
            delay_after_startup: std::time::Duration::ZERO,
        };
        let frigate_client = make_frigate_client(config).unwrap();

        frigate_client.test_call().await.unwrap();

        assert_eq!(login_count.load(Ordering::SeqCst), 2);
        assert_eq!(summary_count.load(Ordering::SeqCst), 2);
    }

    async fn spawn_test_server<F>(handler: F) -> String
    where
        F: Fn(&str) -> String + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handler = Arc::new(handler);

        let _server_task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let handler = handler.clone();
                let _connection_task = tokio::spawn(async move {
                    let mut buffer = [0; 4096];
                    let bytes_read = stream.read(&mut buffer).await.unwrap();
                    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
                    let response = handler(&request);
                    stream.write_all(response.as_bytes()).await.unwrap();
                });
            }
        });

        format!("http://{address}")
    }

    fn login_response() -> String {
        String::from(
            "HTTP/1.1 200 OK\r\n\
             Set-Cookie: frigate_token=token; Path=/; HttpOnly\r\n\
             Content-Length: 0\r\n\
             Connection: close\r\n\r\n",
        )
    }

    fn json_response(body: &str) -> String {
        let content_length = body.len();
        format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {content_length}\r\n\
             Connection: close\r\n\r\n\
             {body}",
        )
    }

    fn response(status: u16, body: &str) -> String {
        let content_length = body.len();
        let reason = match status {
            401 => "Unauthorized",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "OK",
        };
        format!(
            "HTTP/1.1 {status} {reason}\r\n\
             Content-Length: {content_length}\r\n\
             Connection: close\r\n\r\n\
             {body}",
        )
    }
}
