use file_sender::{path_descriptor::PathDescriptor, traits::StoreDestination};
use std::sync::Arc;

use crate::system::traits::FileSenderMaker;

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

pub async fn make_file_senders<S: FileSenderMaker>(
    file_sender_maker: &Arc<S>,
    remaining_path_descriptors: &[Arc<PathDescriptor>],
) -> Vec<FileSenderOrPathDescriptor> {
    let result =
        remaining_path_descriptors
            .iter()
            .map(|d| (d.clone(), (file_sender_maker)(d)))
            .map(|(descriptor, sender_result)| match sender_result {
                Ok(s) => s.into(),
                Err(e) => {
                    tracing::error!(
                        "Failed to create file sender with descriptor `{descriptor}`: {e}",
                    );
                    descriptor.into()
                }
            })
            .collect::<Vec<_>>();

    // Initialize file senders that were successfully opened
    for sender in &result {
        if let FileSenderOrPathDescriptor::FileSender(s) = sender {
            match s.init().await {
                Ok(()) => tracing::trace!(
                    "Initializing file sender with descriptor `{}` is successful.",
                    s.path_descriptor()
                ),
                Err(e) => tracing::error!(
                    "Error while initializing file sender after successful creation. Path descriptor: `{}`. Error: {e}",
                    s.path_descriptor()
                ),
            }
        }
    }

    result
}
