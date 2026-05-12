//! tracing layer that scrubs `SensitiveString` and pattern matched secrets
//! out of event field values.
//!
//! Phase 2 task 2.18 fills this in with the real implementation. The stub
//! exists today so the public crate surface is stable.

use tracing::Subscriber;
use tracing_subscriber::layer::{Context, Layer};

/// Placeholder. Phase 2.18 replaces with a real visitor that intercepts
/// `record` calls and runs `Redactor::all` on string field values before
/// the next layer formats them. Today this layer is a no op.
#[derive(Default)]
pub struct RedactingLayer;

impl<S: Subscriber> Layer<S> for RedactingLayer {
    // Phase 2.18: implement on_event with a field visitor that scrubs.
    fn enabled(&self, _: &tracing::Metadata<'_>, _: Context<'_, S>) -> bool {
        true
    }
}
