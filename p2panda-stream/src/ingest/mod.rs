// SPDX-License-Identifier: MIT OR Apache-2.0

//! Checks an incoming operation for log integrity and persists it into the store when valid.
mod args;
mod ooo;
mod operation;
mod processor;

pub use args::IngestArgs;
pub use ooo::{OooBuffer, OooResult};
pub use operation::{IngestError, IngestResult, ingest_operation, validate_operation};
pub use processor::Ingest;
