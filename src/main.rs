use clap::Parser;
use options::run_options::{self, RunOptions};
use sync_system::runner::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = RunOptions::parse();

    match args.command {
        run_options::RunCommand::Start(start_options) => run(start_options).await,
    }
}
