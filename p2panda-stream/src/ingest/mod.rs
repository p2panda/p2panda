// SPDX-License-Identifier: MIT OR Apache-2.0

mod operation;
mod processor;

pub use operation::{IngestError, ingest_operation};
pub use processor::{Ingest, IngestArguments};
