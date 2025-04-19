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

pub trait AsyncFileSenderResult:
    Future<Output = anyhow::Result<Box<dyn StoreDestination<Error = anyhow::Error>>>> + Send + 'static
{
}

impl<F> AsyncFileSenderResult for F where
    F: Future<Output = anyhow::Result<Box<dyn StoreDestination<Error = anyhow::Error>>>>
        + Send
        + 'static
{
}

pub trait FileSenderMaker<F: AsyncFileSenderResult>:
    Fn(Arc<PathDescriptor>) -> F + Send + Sync + 'static
{
}

impl<T, F: AsyncFileSenderResult> FileSenderMaker<F> for T where
    T: Fn(Arc<PathDescriptor>) -> F + Send + Sync + 'static
{
}
