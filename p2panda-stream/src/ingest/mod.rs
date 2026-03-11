// SPDX-License-Identifier: MIT OR Apache-2.0

mod args;
mod operation;
mod processor;

pub use args::IngestArgs;
pub use operation::{IngestError, ingest_operation};
pub use processor::{Ingest, IngestResult};
