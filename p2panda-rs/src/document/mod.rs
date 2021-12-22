// SPDX-License-Identifier: AGPL-3.0-or-later

//! A Document is a resolvable data type which is made up of a linked graph of operations. Documents MUST have a single root ‘CREATE’
//! operation. All other operations which mutate the initial data are inserted from this point and, due to the nature of operations,
//! connect together to form a directed acyclic graph.
//!
//! The graph MUST contain only one root operation and there MUST be a path from the root to every other Operation contained in this
//! Document. All Operations MUST contain the hash id of both the Document it is operating on as well the previous known operation.
//! Documents MUST implement a method for topologically sorting the graph, iterating over the ordered list of operations, and applying
//! all updates onto an Instance following the document schema. This process MUST be deterministic, any Document replicas which
//! contain the same Operations MUST resolve to the same value.
//!
//! All operations in a document MUST follow the documents Schema definition. This is defined by the root CREATE operation.
#[allow(clippy::module_inception)]
mod document;
mod error;

pub use document::{Document, DocumentBuilder};
pub use error::{DocumentBuilderError, DocumentError};
