// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::identity::{PublicKey, Signature};
use crate::operation::body::EncodedBody;
use crate::operation::header::error::ValidateHeaderError;
use crate::operation::header::{EncodedHeader, Header};
use crate::operation::header::traits::Authored;

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
pub fn validate_payload(header: &Header, payload: &EncodedBody) -> Result<(), ValidateHeaderError> {
    if header.payload_hash() != &payload.hash() {
        return Err(ValidateHeaderError::PayloadHashMismatch);
    }

    if header.payload_size() != payload.size() {
        return Err(ValidateHeaderError::PayloadSizeMismatch);
    }

    Ok(())
}
