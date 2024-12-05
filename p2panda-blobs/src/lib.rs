// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blobs service offering storage, retrieval and synchronisation of content-addressed data.
//!
//! `p2panda-blobs` relies on the
//! [`iroh-blobs`](https://docs.rs/iroh-blobs/latest/iroh_blobs/index.html) crate and offers an API
//! to import blobs into a store and use the resulting BLAKE3 hashes to address them for downloads.
//! In-memory and filesystem-based store options are provided.
//!
//! The blobs service integrates with `p2panda-net` to provide a means of synchronising files
//! between devices using BLAKE3 verified streaming. Memory usage is generally low, even when
//! transferring very large files.
mod blobs;
mod config;
mod download;
mod export;
mod import;
mod protocol;

use iroh_blobs::store;

pub use blobs::Blobs;
pub use config::Config;
pub use download::DownloadBlobEvent;
pub use import::ImportBlobEvent;
pub use protocol::{BlobsProtocol, BLOBS_ALPN};

/// In-memory storage database with support for partial blobs.
pub type MemoryStore = store::mem::Store;

/// Filesystem storage database backed by [redb](https://crates.io/crates/redb) for small blobs and
/// files for large blobs.
pub type FilesystemStore = store::fs::Store;
