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
    async fn test_call(base_url: String) {
        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
        };
        let frigate_client = make_frigate_client(config, None).unwrap();
        frigate_client.test_call().await.unwrap();
    }

    #[tokio::test]
    #[rstest]
    async fn basic(base_url: String) {
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
}
