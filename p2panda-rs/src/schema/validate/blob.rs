// SPDX-License-Identifier: AGPL-&3.0-or-later

use once_cell::sync::Lazy;
use regex::Regex;

use crate::operation::plain::{PlainFields, PlainValue};
use crate::schema::validate::error::BlobError;

/// Checks "mime_type" field of operations with "blob_v1" schema id.
///
/// 1. It matches expected mime type format
pub fn validate_mime_type(value: &str) -> bool {
    static NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
        // Unwrap as we checked the regular expression for correctness
        Regex::new("^[a-z-]{1,12}\\/(([a-z0-9]{1,18})[\\.|+|-]){0,6}[a-z0-9]{1,16}$").unwrap()
    });

    NAME_REGEX.is_match(value)
}

/// Validate formatting for operations following `blob_v1` system schemas.
///
/// These operations contain a "length", "mime_type" and "pieces" field some of which have special
/// limitations defined by the p2panda specification.
///
/// Please note that this does not check type field type or the operation fields in general, as
/// this should be handled by other validation methods. This method is only checking the
/// special requirements of this particular system schema.
pub fn validate_blob_v1_fields(fields: &PlainFields) -> Result<(), BlobError> {
    // `pieces` and `length` fields don't have any special requirements.

    // Check "mime_type" field
    let blob_mime_type = fields.get("mime_type");

    match blob_mime_type {
        Some(PlainValue::String(value)) => {
            if validate_mime_type(value) {
                Ok(())
            } else {
                Err(BlobError::MimeTypeInvalid)
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
    use crate::test_utils::fixtures::random_document_view_id;

    use super::{validate_blob_v1_fields, validate_mime_type};

    #[rstest]
    #[case(vec![
       ("length", 1.into()),
       ("mime_type", "image/png".into()),
       ("pieces", vec![random_document_view_id()].into()),
    ].into())]
    #[case(vec![
        ("length", 1000.into()),
        ("mime_type", "application/x-zip-compressed".into()),
        ("pieces", vec![random_document_view_id()].into()),
     ].into())]
    #[case(vec![
        ("length", 99999.into()),
        ("mime_type", "application/vnd.openxmlformats-officedocument.presentationml.slideshow".into()),
        ("pieces", vec![random_document_view_id()].into()),
     ].into())]
    #[should_panic]
    #[case(vec![
        ("length", 100.into()),
        ("mime_type", "not a mime type".into()),
        ("pieces", vec![random_document_view_id()].into()),
     ].into())]
    fn check_fields(#[case] fields: PlainFields) {
        assert!(validate_blob_v1_fields(&fields).is_ok());
    }

    #[rstest]
    #[case("video/webm")]
    #[case("image/webp")]
    #[case("x-conference/x-cooltalk")]
    #[case("application/vnd.cluetrust.cartomobile-config-pkg")]
    #[case("application/emma+xml")]
    #[case("my/made.up.mime.type")] // This still passes....
    #[should_panic]
    #[case("wrong format")]
    #[should_panic]
    #[case("wrong_format")]
    #[should_panic]
    #[case("wrong/f o r m a t")]
    #[should_panic]
    #[case("wrong/!format!")]
    #[should_panic]
    #[case("wrong/for..mat")]
    #[should_panic]
    #[case("wro.ng/for..mat")]
    #[should_panic]
    #[case("this/mime.type.has.one.too.many.elements.yes")]
    #[should_panic]
    #[case("this/mime.type.hasoneelementwhichiswaytoolong")]
    #[should_panic]
    #[case("thismimetypealsohasonelongelement/one.element.too.long")]
    fn check_mime_type_field(#[case] name_str: &str) {
        assert!(validate_mime_type(name_str));
    }
}
