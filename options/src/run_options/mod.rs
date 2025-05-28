pub mod start_options;

use clap::{Parser, Subcommand};

const DEFAULT_CONFIG_FILE_PATH: &str = "config.yaml";

#[derive(Parser)]
pub struct RunOptions {
    #[clap(subcommand)]
    pub command: RunCommand,
}

#[derive(Subcommand, Clone, Debug)]
pub enum RunCommand {
    /// The default command to start the application.
    Start(start_options::StartOptions),
}
