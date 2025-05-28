use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Clone, Debug, Default)]
pub struct StartOptions {
    /// The path to the config file
    /// If not provided, the default value is used, config.yaml
    #[clap(long, short('c'), default_value_os = super::DEFAULT_CONFIG_FILE_PATH)]
    pub config_file_path: PathBuf,
}
