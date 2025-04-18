use std::sync::Arc;

use file_sender::{path_descriptor::PathDescriptor, traits::StoreDestination};

pub enum FileSenderOrPathDescriptor {
    // Represent a successful establishment of the sender
    FileSender(Box<dyn StoreDestination<Error = anyhow::Error>>),
    // Represent a still pending establishment of the sender
    PathDescriptor(Arc<PathDescriptor>),
}

impl From<Box<dyn StoreDestination<Error = anyhow::Error>>> for FileSenderOrPathDescriptor {
    fn from(sender: Box<dyn StoreDestination<Error = anyhow::Error>>) -> Self {
        FileSenderOrPathDescriptor::FileSender(sender)
    }
}

impl From<Arc<PathDescriptor>> for FileSenderOrPathDescriptor {
    fn from(d: Arc<PathDescriptor>) -> Self {
        FileSenderOrPathDescriptor::PathDescriptor(d)
    }
}

#[allow(clippy::type_complexity)]
pub fn split_file_senders_and_descriptors(
    iter: impl IntoIterator<Item = FileSenderOrPathDescriptor>,
) -> (
    Vec<Box<dyn StoreDestination<Error = anyhow::Error>>>,
    Vec<Arc<PathDescriptor>>,
) {
    let mut d = Vec::new();
    let mut s = Vec::new();
    iter.into_iter().for_each(|v| match v {
        FileSenderOrPathDescriptor::FileSender(store_destination) => s.push(store_destination),
        FileSenderOrPathDescriptor::PathDescriptor(path_descriptor) => d.push(path_descriptor),
    });
    (s, d)
}
