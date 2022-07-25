// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::entry::EncodedEntry;
use crate::identity::Author;
use crate::operation::EncodedOperation;

/// A request to retrieve the next entry args for an author and document.
pub trait AsEntryArgsRequest {
    /// Returns the Author.
    fn author(&self) -> &Author;

    /// Returns the document id.
    fn document_id(&self) -> &Option<DocumentId>;
}

/// A request to publish a new entry.
pub trait AsPublishEntryRequest {
    /// Returns the encoded entry.
    fn entry_signed(&self) -> &EncodedEntry;

    /// Returns the encoded operation.
    ///
    /// Currently not optional.
    fn operation_encoded(&self) -> &EncodedOperation;
}
