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

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::DocumentId;
    use crate::identity::{Author, KeyPair};
    use crate::storage_provider::traits::test_utils::EntryArgsRequest;
    use crate::test_utils::fixtures::{document_id, key_pair};
    use crate::Validate;

    #[rstest]
    fn validates(key_pair: KeyPair, document_id: DocumentId) {
        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();

        let entry_args_request = EntryArgsRequest {
            author: author.clone(),
            document: None,
        };

        assert!(entry_args_request.validate().is_ok());

        let entry_args_request = EntryArgsRequest {
            author,
            document: Some(document_id),
        };

        assert!(entry_args_request.validate().is_ok());
    }
}
