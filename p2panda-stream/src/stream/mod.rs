// SPDX-License-Identifier: MIT OR Apache-2.0

mod decode;
#[allow(dead_code)]
mod dependencies;
mod ingest;

pub use decode::{Decode, DecodeExt};
pub use ingest::{Ingest, IngestExt};
