// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation::error::ValidateHeaderExtensionsError;
use crate::operation::traits::Actionable;
use crate::operation::header::{Header, HeaderExtension};
use crate::operation::OperationAction;
use crate::Validate;

/// This method validates a headers extensions against those we require for a valid p2panda
/// operation. 
pub fn validate_header_extensions(header: &Header) -> Result<(), ValidateHeaderExtensionsError> {
    let HeaderExtension {
        previous,
        timestamp,
        backlink,
        depth,
        ..
    } = &header.4;

    // Perform basic header validation
    header.validate()?;

    // All operations require a timestamp
    if timestamp.is_none() {
        return Err(ValidateHeaderExtensionsError::ExpectedTimestamp);
    }

    // All operations require a depth
    let depth = match depth {
        Some(depth) => depth,
        None => return Err(ValidateHeaderExtensionsError::ExpectedDepth),
    };

    match header.action() {
        // Operations with no action set in their header and without a document id are CREATE operations.
        OperationAction::Create => {
            if backlink.is_some() {
                return Err(ValidateHeaderExtensionsError::UnexpectedBacklink);
            }

            if previous.is_some() {
                return Err(ValidateHeaderExtensionsError::UnexpectedPreviousOperations);
            }

            if *depth != 0 {
                return Err(ValidateHeaderExtensionsError::ExpectedZeroDepth);
            }
            Ok(())
        }
        // Operations with the document id set are either UPDATE or DELETE operations.
        OperationAction::Update | OperationAction::Delete => {
            if previous.is_none() {
                return Err(ValidateHeaderExtensionsError::ExpectedPreviousOperations);
            }

            if *depth == 0 {
                return Err(ValidateHeaderExtensionsError::ExpectedNonZeroDepth);
            }
            Ok(())
        }
    }
}
