use std::future::Future;

use tokio::task::JoinHandle;
use tracing::{Instrument, Span};

pub fn spawn_in_current_span<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future.in_current_span())
}

pub fn spawn_in_span<F>(future: F, span: Span) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future.instrument(span))
}
