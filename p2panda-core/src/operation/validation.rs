// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::hash::Hash;
use crate::identity::VerifyingKey;
use crate::operation::OperationError;
use crate::traits::{Chain, Digest, Offchain, Provenance};

/// Validate the header and body (when provided) of a single operation. All basic header validation
/// is performed (identical to [`validate_header`]()) and additionally the body bytes hash and size
/// are checked to be correct.
///
/// This method validates that the following conditions are true:
/// * Signature can be verified against the author public key and unsigned header bytes
/// * If `payload_hash` is set the `payload_size` is > `0` otherwise it is zero
/// * If `backlink` is set then `seq_num` is > `0` otherwise it is zero
/// * If provided the body bytes hash and size match those claimed in the header
pub fn validate_operation<T>(operation: &T) -> Result<(), OperationError>
where
    T: Provenance<VerifyingKey> + Chain<Hash> + Offchain<Hash>,
{
    validate_header::<T>(operation)?;

    let claimed_payload_size = operation.payload_size();
    let claimed_payload_hash: Option<Hash> = match claimed_payload_size {
        0 => None,
        _ => {
            let hash = operation
                .payload_hash()
                .ok_or(OperationError::MissingPayloadHash)?;
            Some(hash)
        }
    };

    if let Some(body) = &operation.payload()
        && (claimed_payload_hash != Some(body.hash()) || claimed_payload_size != body.size())
    {
        return Err(OperationError::PayloadMismatch);
    }

    Ok(())
}

/// Validate an operation header.
///
/// This method validates that the following conditions are true:
/// * Signature can be verified against the author public key and unsigned header bytes
/// * If `payload_hash` is set the `payload_size` is > `0` otherwise it is zero
/// * If `backlink` is set then `seq_num` is > `0` otherwise it is zero
pub fn validate_header<T>(header: &T) -> Result<(), OperationError>
where
    T: Provenance<VerifyingKey> + Chain<Hash> + Offchain<Hash>,
{
    if !header.verify() {
        return Err(OperationError::SignatureMismatch);
    }

    if (header.payload_hash().is_some() && header.payload_size() == 0)
        || (header.payload_hash().is_none() && header.payload_size() > 0)
    {
        return Err(OperationError::InconsistentPayloadInfo);
    }

    if header.backlink().is_some() && header.seq_num() == 0 {
        return Err(OperationError::SeqNumMismatch);
    }

    if header.backlink().is_none() && header.seq_num() > 0 {
        return Err(OperationError::BacklinkMissing);
    }

    Ok(())
}

/// Validate a backlink contained in a header against a past header which is assumed to have been
/// retrieved from a local store.
///
/// This method validates that the following conditions are true:
/// * Current and past headers contain the same public key
/// * Current headers seq number increments from the past one by exactly `1`
/// * Backlink hash contained in the current header matches the hash of the past header
pub fn validate_backlink<T>(past_header: &T, header: &T) -> Result<(), OperationError>
where
    T: Provenance<VerifyingKey> + Digest<Hash> + Chain<Hash>,
{
    if past_header.author() != header.author() {
        return Err(OperationError::TooManyAuthors);
    }

    if past_header.seq_num() + 1 != header.seq_num() {
        return Err(OperationError::SeqNumNonIncremental(
            past_header.seq_num() + 1,
            header.seq_num(),
        ));
    }

    match header.backlink() {
        Some(backlink) => {
            if past_header.hash() != backlink {
                return Err(OperationError::BacklinkMismatch);
            }
        }
        None => {
            return Err(OperationError::BacklinkMissing);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::hash::Hash;
    use crate::identity::SigningKey;
    use crate::operation::{Body, Header, Operation, OperationError};

    use super::{validate_backlink, validate_header, validate_operation};

    #[test]
    fn sign_and_verify() {
        let signing_key = SigningKey::generate();
        let body = Body::from_bytes("Hello, Sloth!".as_bytes());

        type CustomExtensions = (u32, String);

        let header = Header::<CustomExtensions>::builder()
            .body(&body)
            .build(&signing_key, (42, "penguin".to_string()));
        assert!(header.verify());

        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };
        assert!(validate_operation::<Operation<_>>(&operation).is_ok());
    }

    #[test]
    fn valid_backlink_header() {
        let signing_key = SigningKey::generate();

        let header_0 = Header::builder().build(&signing_key, ());
        assert!(validate_header(&header_0).is_ok());

        let header_1 = Header::builder()
            .chain(1, header_0.hash())
            .build(&signing_key, ());
        assert!(validate_header(&header_1).is_ok());

        assert!(validate_backlink(&header_0, &header_1).is_ok());
    }

    #[test]
    fn invalid_operations() {
        let signing_key = SigningKey::generate();
        let body = Body::from_bytes("Hello, Sloth!".as_bytes());

        let header_base = Header::<()> {
            verifying_key: signing_key.verifying_key(),
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            ..Default::default()
        };

        // Signature doesn't match public key
        let mut header = header_base.clone();
        header.verifying_key = SigningKey::generate().verifying_key();
        header.sign(&signing_key);
        std::assert_matches!(
            validate_header(&header),
            Err(OperationError::SignatureMismatch)
        );

        // Backlink missing
        let mut header = header_base.clone();
        header.seq_num = 1;
        header.sign(&signing_key);
        std::assert_matches!(
            validate_header(&header),
            Err(OperationError::BacklinkMissing)
        );

        // Backlink given but sequence number indicates none
        let mut header = header_base.clone();
        header.backlink = Some(Hash::digest(vec![4, 5, 6]));
        header.sign(&signing_key);
        std::assert_matches!(
            validate_header(&header),
            Err(OperationError::SeqNumMismatch)
        );

        // Payload size does not match
        let mut header = header_base.clone();
        header.payload_size = 11;
        header.sign(&signing_key);
        std::assert_matches!(
            validate_operation(&Operation {
                hash: header.hash(),
                header,
                body: Some(body.clone()),
            }),
            Err(OperationError::PayloadMismatch)
        );

        // Payload hash does not match
        let mut header = header_base.clone();
        header.payload_hash = Some(Hash::digest(vec![4, 5, 6]));
        header.sign(&signing_key);
        std::assert_matches!(
            validate_operation(&Operation {
                hash: header.hash(),
                header,
                body: Some(body.clone()),
            }),
            Err(OperationError::PayloadMismatch)
        );
    }
}
