use sync_system::runner::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await?;

    Ok(())
}
