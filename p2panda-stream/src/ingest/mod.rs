// SPDX-License-Identifier: MIT OR Apache-2.0

mod operation;
mod processor;
mod traits;

pub use operation::{IngestError, ingest_operation};
pub use processor::Ingest;
pub use traits::IngestArgs;
