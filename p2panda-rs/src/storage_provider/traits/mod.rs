// SPDX-License-Identifier: AGPL-3.0-or-later

//! Traits used when implementing a custom storage provider for a p2panda client or node.
mod document_store;
mod entry_store;
mod log_store;
mod models;
mod operation_store;

pub use document_store::DocumentStore;
pub use entry_store::EntryStore;
pub use log_store::LogStore;
pub use models::EntryWithOperation;
pub use operation_store::OperationStore;
