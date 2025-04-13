use crate::json::review::Review;
use async_trait::async_trait;

#[async_trait]
pub trait FrigateApi {
    /// Attempt a call to the API that only tests whether the API is healthy
    #[must_use]
    async fn test_call(&self) -> anyhow::Result<()>;

    /// Returns review information as json
    /// https://docs.frigate.video/integrations/api/get-review-review-review-id-get
    /// https://demo.frigate.video/api/review/:review_id
    #[must_use]
    async fn review(&self, id: &str) -> anyhow::Result<Review>;

    /// Returns MP4 clip as raw data
    /// Ok(None) is returned if the request is successful, but the video file is empty (zero bytes).
    /// https://docs.frigate.video/integrations/api/recording-clip-camera-name-start-start-ts-end-end-ts-clip-mp-4-get/
    /// https://demo.frigate.video/api/:camera_name/start/:start_ts/end/:end_ts/clip.mp4
    #[must_use]
    async fn recording_clip(
        &self,
        camera_label: &str,
        start_ts: f64,
        end_ts: f64,
    ) -> anyhow::Result<Option<Vec<u8>>>;
}
