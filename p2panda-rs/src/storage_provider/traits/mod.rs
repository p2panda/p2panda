// SPDX-License-Identifier: AGPL-3.0-or-later

//! Traits describing the interfaces which should be implemented by any storage
//! layer intended to be used by a p2panda peer. With these traits implemented
//! one can store and query `Entries` and `Operations` and query `Logs` and
//! `Documents`.
//!
//! The primary data types which require persisting are `Entry` and `Operation`. These
//! are the immutable objects which peers publish and replicate. Their storage methods  
//! are described in [`EntryStore`] and [`OperationStore`]. Entries are associated with
//! logs, which can be queried via the [`LogStore`].
//!
//! [`DocumentStore`] outlines a basic API for querying documents. Documents are
//! entirely derived from their respective operations, a documents' current state, as
//! well as any historic views which need to be retained, can be seen as a caching
//! layer on top of the persisted, immutable operations. Efficient storage and
//! querying of documents should be considered by implementers of these traits.
mod document_store;
mod operation_store;

pub use document_store::DocumentStore;
pub use operation_store::OperationStore;
