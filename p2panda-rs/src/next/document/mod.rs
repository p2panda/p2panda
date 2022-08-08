// SPDX-License-Identifier: AGPL-3.0-or-later

//! Document is a replicatable data type designed to handle concurrent updates in a way where all
//! replicas eventually resolve to the same deterministic value.
//!
//! A Document is made up of a linked graph of operations. During a process of ordering and
//! reduction the graph is resolved to a single data item matching the documents schema definition.
//! Any two documents (replicas) which contain the same collection of operations will resolve to
//! the same value.
//!
//! In the p2panda network, Documents are materialised on nodes and the resulting document views
//! are stored in the database.
// @TODO: Bring back doc-string example here
#[allow(clippy::module_inception)]
mod document;
mod document_id;
mod document_view;
mod document_view_fields;
mod document_view_hash;
mod document_view_id;
pub mod error;
pub mod materialization;

pub use document::{Document, DocumentBuilder, IsDeleted, IsEdited};
pub use document_id::DocumentId;
pub use document_view::DocumentView;
pub use document_view_fields::{DocumentViewFields, DocumentViewValue};
pub use document_view_hash::DocumentViewHash;
pub use document_view_id::DocumentViewId;
