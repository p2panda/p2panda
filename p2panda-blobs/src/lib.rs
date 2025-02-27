// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(doctest, doc=include_str!("../README.md"))]

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

use iroh::{NodeAddr as IrohNodeAddr, NodeId};
use iroh_blobs::store;

pub use blobs::Blobs;
pub use config::Config;
pub use download::DownloadBlobEvent;
pub use import::ImportBlobEvent;
use p2panda_net::NodeAddress;
pub use protocol::{BLOBS_ALPN, BlobsProtocol};

/// In-memory storage database with support for partial blobs.
pub type MemoryStore = store::mem::Store;

/// Filesystem storage database backed by [redb](https://crates.io/crates/redb) for small blobs and
/// files for large blobs.
pub type FilesystemStore = store::fs::Store;

/// Converts a `p2panda-net` node address type to the `iroh` implementation.
pub(crate) fn from_node_addr(addr: NodeAddress) -> IrohNodeAddr {
    let node_id = NodeId::from_bytes(addr.public_key.as_bytes()).expect("invalid public key");
    let mut node_addr =
        IrohNodeAddr::new(node_id).with_direct_addresses(addr.direct_addresses.to_vec());
    if let Some(url) = addr.relay_url {
        node_addr = node_addr.with_relay_url(url.into());
    }
    node_addr
}
