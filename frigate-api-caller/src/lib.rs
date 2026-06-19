pub mod config;
pub mod helpers;
pub mod json;
pub mod traits;

use crate::json::stats::{Stats, StatsProps};
use anyhow::Context;
use async_trait::async_trait;
use config::FrigateApiConfig;
use json::review::Review;
use serde_json::Value;
use std::sync::Arc;
use tracing::trace_span;
use traits::FrigateApi;

pub fn make_frigate_client(config: FrigateApiConfig) -> anyhow::Result<Arc<dyn FrigateApi>> {
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

    Ok(Arc::new(result))
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

    async fn stats(&self) -> anyhow::Result<Box<dyn StatsProps>> {
        let base_url = &self.config.frigate_api_base_url;
        let url = format!("{base_url}/api/stats");
        let request = self
            .client
            .request(reqwest::Method::GET, url)
            .headers(json_headers_map());
        let response = request.send().await?;
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
    use test_utils::random::{Seed, gen_random_bytes, make_seedable_rng, random_seed};

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

    #[tokio::test]
    #[rstest]
    #[trace]
    #[ignore = "If you want to run this, set the fixture url then run it"]
    async fn test_call(base_url: String) {
        let config = FrigateApiConfig {
            frigate_api_base_url: base_url,
            frigate_api_proxy: None,
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
