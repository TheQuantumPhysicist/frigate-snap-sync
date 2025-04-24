use async_trait::async_trait;
use frigate_api_caller::json::review::Review;
use frigate_api_caller::traits::FrigateApi;

#[must_use]
pub fn make_frigate_client_mock() -> Box<dyn FrigateApi> {
    Box::new(MockFrigateApi::new())
}

mockall::mock! {
    pub FrigateApi {}

    #[async_trait]
    impl FrigateApi for FrigateApi {
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
