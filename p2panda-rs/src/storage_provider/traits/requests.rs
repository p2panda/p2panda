// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::entry::EntrySigned;
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::Validate;

/// A request to retrieve the next entry args for an author and document.
pub trait AsEntryArgsRequest: Validate {
    /// Returns the Author parameter.
    fn author(&self) -> &Author;

    /// Returns the document id parameter.
    fn document_id(&self) -> &Option<DocumentId>;
}

/// A request to publish a new entry.
pub trait AsPublishEntryRequest: Validate {
    /// Returns the EntrySigned parameter
    fn entry_signed(&self) -> &EntrySigned;

    /// Returns the OperationEncoded parameter
    ///
    /// Currently not optional.
    fn operation_encoded(&self) -> &OperationEncoded;
}
