// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::entry::EntrySigned;
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::storage_provider::ValidationError;
use crate::Validate;

/// Request body of `panda_getEntryArguments`.
pub trait AsEntryArgsRequest {
    /// Returns the Author parameter.
    fn author(&self) -> &Author;

    /// Returns the document id Hash parameter.
    fn document(&self) -> &Option<DocumentId>;

    /// Validates the `EntryArgument` parameters
    fn validate(&self) -> Result<(), ValidationError> {
        // Validate `author` request parameter
        self.author().validate()?;

        // Validate `document` request parameter when it is set
        match self.document() {
            None => (),
            Some(doc) => {
                doc.validate()?;
            }
        };
        Ok(())
    }
}

/// Request body of `panda_publishEntry`.
pub trait AsPublishEntryRequest {
    /// Returns the EntrySigned parameter
    fn entry_encoded(&self) -> &EntrySigned;

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
    use crate::storage_provider::traits::AsEntryArgsRequest;
    use crate::test_utils::fixtures::{document_id, key_pair};

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
