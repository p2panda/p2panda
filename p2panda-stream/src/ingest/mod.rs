// SPDX-License-Identifier: MIT OR Apache-2.0

//! Checks an incoming operation for log integrity and persists it into the store when valid.
mod args;
mod operation;
mod processor;

pub use args::IngestArgs;
pub use operation::{IngestError, ingest_operation};
pub use processor::{Ingest, IngestResult};
