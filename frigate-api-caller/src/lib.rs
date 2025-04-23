pub mod config;
pub mod helpers;
pub mod json;
pub mod mocks;
pub mod traits;

use anyhow::Context;
use async_trait::async_trait;
use config::FrigateApiConfig;
use json::review::Review;
use serde_json::Value;
use tracing::trace_span;
use traits::FrigateApi;

pub fn make_frigate_client(config: FrigateApiConfig) -> anyhow::Result<Box<dyn FrigateApi>> {
    let span = trace_span!("make_frigate_client");
    let _enter = span.enter();

    tracing::trace!("Begin make_frigate_client function");
    let builder = reqwest::ClientBuilder::new();

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

    Ok(Box::new(result))
}

struct FrigateApiClient {
    client: reqwest::Client,
    config: FrigateApiConfig,
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

        let request = self
            .client
            .request(reqwest::Method::GET, &url)
            .headers(json_headers_map());

        tracing::trace!("Submitting request to URL: {url}");
        let response = request
            .send()
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
        let request = self
            .client
            .request(reqwest::Method::GET, url)
            .headers(json_headers_map());
        let response = request.send().await?;
        let result = response.json::<Review>().await?;

        tracing::debug!("Call `review` with id {id} with response: {:?}", result);

        Ok(result)
    }

    async fn recording_clip(
        &self,
        camera_label: &str,
        start_ts: f64,
        end_ts: f64,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let base_url = &self.config.frigate_api_base_url;
        let url = format!("{base_url}/api/{camera_label}/start/{start_ts}/end/{end_ts}/clip.mp4");
        let request = self
            .client
            .request(reqwest::Method::GET, url)
            .headers(json_headers_map());
        let response = request.send().await?;
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

    #[fixture]
    pub fn base_url() -> String {
        "http://127.0.0.1:5000".to_string()
    }

    #[tokio::test]
    #[rstest]
    #[ignore = "If you want to run this, set the fixture url then run it"]
    async fn test_call(base_url: String) {
        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
        };
        let frigate_client = make_frigate_client(config).unwrap();
        frigate_client.test_call().await.unwrap();
    }

    #[tokio::test]
    #[rstest]
    #[ignore = "If you want to run this, set the fixture url, set the parameters then run it"]
    async fn review(base_url: String) {
        let review_id = "1744534711.333822-vsz5s4";

        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
        };
        let frigate_client = make_frigate_client(config).unwrap();
        println!(
            "Review: {:?}",
            frigate_client.review(review_id).await.unwrap()
        );
    }

    #[tokio::test]
    #[rstest]
    #[ignore = "If you want to run this, set the fixture url, set the parameters then run it"]
    async fn recording_clip(base_url: String) {
        let camera_label = "my_camera";
        let start_timestamp = 1744534711.333822;
        let end_timestamp = 1744534731.13457;

        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
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
