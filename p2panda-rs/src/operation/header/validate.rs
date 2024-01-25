// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::{DocumentId, DocumentViewId};
use crate::identity::{PublicKey, Signature};
use crate::operation::body::EncodedBody;
use crate::operation::header::error::ValidateHeaderError;
use crate::operation::header::{EncodedHeader, Header};
use crate::operation::traits::Verifiable;

use super::error::DocumentLinksError;
use super::DocumentLinks;

/// Checks if the operation is authentic by verifying the public key with the given signature
/// (#E5).
pub fn verify_signature(
    public_key: &PublicKey,
    signature: &Signature,
    encoded_header: &EncodedHeader,
) -> Result<(), ValidateHeaderError> {
    if !PublicKey::verify(public_key, &encoded_header.unsigned_bytes(), &signature) {
        todo!()
    };

    Ok(())
}

/// Checks if the claimed payload hash and size matches the actual data (#E6).
pub fn verify_payload(header: &Header, payload: &EncodedBody) -> Result<(), ValidateHeaderError> {
    if header.payload_hash() != &payload.hash() {
        return Err(ValidateHeaderError::PayloadHashMismatch);
    }

    if header.payload_size() != payload.size() {
        return Err(ValidateHeaderError::PayloadSizeMismatch);
    }

    Ok(())
}

pub fn validate_document_links(
    document_id: Option<DocumentId>,
    previous: Option<DocumentViewId>,
) -> Result<Option<DocumentLinks>, DocumentLinksError> {
    match (document_id, previous) {
        (Some(document_id), Some(previous)) => Ok(Some(DocumentLinks(document_id, previous))),
        (None, Some(_)) => Err(DocumentLinksError::ExpectedPrevious),
        (Some(_), None) => Err(DocumentLinksError::ExpectedDocumentId),
        (None, None) => Ok(None),
    }
}
