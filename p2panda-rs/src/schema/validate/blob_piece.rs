// SPDX-License-Identifier: AGPL-&3.0-or-later

use crate::operation_v2::body::plain::{PlainFields, PlainValue};
use crate::schema::validate::error::BlobPieceError;

/// Maximum number of bytes a single blob piece can contain.
pub const MAX_BLOB_PIECE_LENGTH: usize = 256 * 1000; // 256kb as per specification

/// Checks "data" field of operations with "blob_piece_v1" schema id.
///
/// 1. It must be less than `MAX_BLOB_PIECE_LENGTH`
pub fn validate_data(value: &Vec<u8>) -> bool {
    value.len() <= MAX_BLOB_PIECE_LENGTH
}

/// Validate formatting for operations following `blob_piece_v1` system schemas.
///
/// These operations contain a "data" field which has special limitations defined by the p2panda specification.
///
/// Please note that this does not check type field type or the operation fields in general, as
/// this should be handled by other validation methods. This method is only checking the
/// special requirements of this particular system schema.
pub fn validate_blob_piece_v1_fields(fields: &PlainFields) -> Result<(), BlobPieceError> {
    // Check "data" field
    let blob_piece_data = fields.get("data");

    match blob_piece_data {
        Some(PlainValue::BytesOrRelation(value)) => {
            if validate_data(value) {
                Ok(())
            } else {
                Err(BlobPieceError::DataInvalid)
            }
        }
        _ => Ok(()),
    }?;

    Ok(())
}

/*#[cfg(test)]
mod test {
    use rstest::rstest;

    use crate::operation::plain::PlainFields;
    use crate::test_utils::generate_random_bytes;

    use super::validate_blob_piece_v1_fields;

    #[rstest]
    #[case(vec![("data", "aGVsbG8gbXkgbmFtZSBpcyBzYW0=".as_bytes().into())].into())]
    #[should_panic]
    #[case(vec![("data", generate_random_bytes(512 * 1000).into())].into())]
    fn check_fields(#[case] fields: PlainFields) {
        assert!(validate_blob_piece_v1_fields(&fields).is_ok());
    }
}*/
