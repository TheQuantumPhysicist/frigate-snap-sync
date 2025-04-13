use crate::json::review::Review;
use async_trait::async_trait;

#[async_trait]
pub trait FrigateApi {
    /// Attempt a call to the API that only tests whether the API is healthy
    #[must_use]
    async fn test_call(&self) -> anyhow::Result<()>;

    #[must_use]
    async fn review(&self, id: &str) -> anyhow::Result<Review>;
}
