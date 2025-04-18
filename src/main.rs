#[tokio::main]
async fn main() -> anyhow::Result<()> {
    sync_system::run().await?;

    Ok(())
}
