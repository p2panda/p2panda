// SPDX-License-Identifier: AGPL-3.0-or-later

//! Temporary module which will guide us through this massive refactoring.
#[allow(clippy::module_inception)]
mod document;
mod document_id;
mod document_view;
mod document_view_fields;
mod document_view_hash;
mod document_view_id;
pub mod error;

pub use document_id::DocumentId;
pub use document_view::DocumentView;
pub use document_view_fields::{DocumentViewFields, DocumentViewValue};
pub use document_view_hash::DocumentViewHash;
pub use document_view_id::DocumentViewId;
