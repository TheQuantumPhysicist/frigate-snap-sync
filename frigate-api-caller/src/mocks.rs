use crate::json::review::Review;
use crate::traits::FrigateApi;
use async_trait::async_trait;

mockall::mock! {
    pub FrigateApiMock {}

    #[async_trait]
    impl FrigateApi for FrigateApiMock {
        async fn test_call(&self) -> anyhow::Result<()>;
        async fn review(&self, id: &str) -> anyhow::Result<Review>;
        async fn recording_clip(
            &self,
            camera_label: &str,
            start_ts: f64,
            end_ts: f64,
        ) -> anyhow::Result<Option<Vec<u8>>>;
    }
}
