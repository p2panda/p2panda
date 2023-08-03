// SPDX-License-Identifier: AGPL-&3.0-or-later

use crate::operation::plain::{PlainFields, PlainValue};
use crate::schema::validate::error::BlobPieceError;

const MAX_BLOB_PIECE_LENGTH: usize = 256;

/// Checks "data" field of operations with "blob_piece_v1" schema id.
///
/// 1. It must be less than `MAX_BLOB_PIECE_LENGTH`
pub fn validate_data(value: &String) -> bool {
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
        Some(PlainValue::StringOrRelation(value)) => {
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

#[cfg(test)]
mod test {
    use rstest::rstest;

    use crate::operation::plain::PlainFields;

    use super::validate_blob_piece_v1_fields;

    #[rstest]
    #[case(vec![
        ("data", "aGVsbG8gbXkgbmFtZSBpcyBzYW0=".into()),
     ].into())]
    #[should_panic]
    #[case(vec![
        ("data", "aGVsbG8gbXkgbmFtZSBpcyBzYW1oZWxsbyBteSBuYW1lIGlzIHNhbWhlbGxvIG15IG5hbW \
                  UgaXMgc2FtaGVsbG8gbXkgbmFtZSBpcyBzYW1oZWxsbyBteSBuYW1lIGlzIHNhbWhlbGxv \
                  G15IG5hbWUgaXMgc2FtaGVsbG8gbXkgbmFtZSBpcyBzYW1oZWxsbyBteSBuYW1lIGlzIHN \
                  hbWhlbGxvIG15IG5hbWUgaXMgc2FtaGVsbG8gbXkgbmFtZSBpcyBzYW1oZWxsbyBteSBuY \
                  W1lIGlzIHNhbWhlbGxvIG15IG5hbWUgaXMgc2FtaGVsbG8gbXkgbmFtZSBpcyBzYW1oZWx \
                  sbyBteSBuYW1lIGlzIHNhbWhlbGxvIG15IG5hbWUgaXMgc2FtaGVsbG8gbXkgbmFtZS".into()),
     ].into())]
    fn check_fields(#[case] fields: PlainFields) {
        assert!(validate_blob_piece_v1_fields(&fields).is_ok());
    }
}
