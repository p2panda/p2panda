// SPDX-License-Identifier: MIT OR Apache-2.0

//! Interfaces and methods to compose event processors.
//!
//! Every processor observes incoming events (as part of a stream) and processes them internally.
//! This usually involves some sort of "materialisation logic" where state is derived from
//! observing these events and enhancing the underlying core data type with additional (security,
//! integrity, etc.) guarantees which can be queried by further layers. Eventually the "enhanced",
//! processed events will be put back on the stream and then further processed by additional layers
//! or forwarded to the application layer.
//!
//! The process itself can be async and buffered, meaning that the resulting items might come out in
//! a different order or may be withheld for a longer time if internal processor requirements are not
//! met.
mod buffered;
mod composed;
mod pipeline;
mod processor;
mod stream;
#[cfg(test)]
mod tests;

pub use composed::{ComposedError, ComposedProcessors};
pub use pipeline::{LayeredBuilder, Pipeline, PipelineBuilder};
pub use processor::Processor;
pub use stream::{ProcessorExt, ProcessorStream, StreamLayerExt};
