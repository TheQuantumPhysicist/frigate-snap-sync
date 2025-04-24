use async_trait::async_trait;
use frigate_api_caller::json::review::Review;
use frigate_api_caller::traits::FrigateApi;

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
