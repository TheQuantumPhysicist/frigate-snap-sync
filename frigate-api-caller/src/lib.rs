pub mod config;
pub mod helpers;
pub mod json;
pub mod traits;

use async_trait::async_trait;
use config::FrigateApiConfig;
use json::review::Review;
use serde_json::Value;
use traits::FrigateApi;

pub fn make_frigate_client(
    config: FrigateApiConfig,
    proxy_address: Option<String>,
) -> anyhow::Result<Box<dyn FrigateApi>> {
    let builder = reqwest::ClientBuilder::new();
    let client = match proxy_address {
        Some(proxy) => builder
            .proxy(reqwest::Proxy::all(proxy).unwrap_or_else(|e| panic!("Invalid proxy URL: {e}")))
            .build()
            .expect("Client builder with proxy failed"),
        None => builder.build().expect("Client builder failed"),
    };

    let result = FrigateApiClient { client, config };

    Ok(Box::new(result))
}

struct FrigateApiClient {
    client: reqwest::Client,
    config: FrigateApiConfig,
}

#[async_trait]
impl FrigateApi for FrigateApiClient {
    async fn test_call(&self) -> anyhow::Result<()> {
        let base_url = &self.config.frigate_api_base_url;
        let url = format!("{base_url}/api/review/summary");
        let request = self
            .client
            .request(reqwest::Method::GET, url)
            .headers(json_headers_map());
        let response = request.send().await?;
        let response_json = response.json::<Value>().await?;

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
        };
        let frigate_client = make_frigate_client(config, None).unwrap();
        frigate_client.test_call().await.unwrap();
    }

    #[tokio::test]
    #[rstest]
    #[ignore = "If you want to run this, set the fixture url, set the parameters then run it"]
    async fn review(base_url: String) {
        let review_id = "1744534711.333822-vsz5s4";

        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
        };
        let frigate_client = make_frigate_client(config, None).unwrap();
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
        };
        let frigate_client = make_frigate_client(config, None).unwrap();
        let mov = frigate_client
            .recording_clip(camera_label, start_timestamp, end_timestamp)
            .await
            .unwrap()
            .unwrap();

        std::fs::write("test.mp4", mov).unwrap();
    }
}
