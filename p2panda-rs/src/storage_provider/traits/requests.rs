// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::next::document::DocumentId;
use crate::next::entry::EncodedEntry;
use crate::next::identity::Author;
use crate::next::operation::EncodedOperation;
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
    /// Returns the EncodedEntry parameter
    fn entry_signed(&self) -> &EncodedEntry;

    /// Returns the OperationEncoded parameter
    ///
    /// Currently not optional.
    fn operation_encoded(&self) -> &EncodedOperation;
}
