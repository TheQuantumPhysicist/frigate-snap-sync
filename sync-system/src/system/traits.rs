use std::sync::Arc;

use file_sender::{path_descriptor::PathDescriptor, traits::StoreDestination};
use frigate_api_caller::{config::FrigateApiConfig, traits::FrigateApi};

pub trait FrigateApiMaker:
    Fn(&FrigateApiConfig) -> anyhow::Result<Box<dyn FrigateApi>> + Send + Sync + 'static
{
}

impl<T> FrigateApiMaker for T where
    T: Fn(&FrigateApiConfig) -> anyhow::Result<Box<dyn FrigateApi>> + Send + Sync + 'static
{
}

pub trait FileSenderMaker:
    Fn(&Arc<PathDescriptor>) -> anyhow::Result<Box<dyn StoreDestination<Error = anyhow::Error>>>
    + Send
    + Sync
    + 'static
{
}

impl<T> FileSenderMaker for T where
    T: Fn(&Arc<PathDescriptor>) -> anyhow::Result<Box<dyn StoreDestination<Error = anyhow::Error>>>
        + Send
        + Sync
        + 'static
{
}
