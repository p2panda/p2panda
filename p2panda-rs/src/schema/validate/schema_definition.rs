// SPDX-License-Identifier: AGPL-&3.0-or-later

use crate::operation::plain::{PlainFields, PlainValue};
use crate::schema::validate::error::SchemaDefinitionError;
use crate::schema::SchemaName;

/// Checks "description" field of operations with "schema_definition_v1" schema id.
///
/// 1. It consists of unicode characters
/// 2. ... and must be at most 256 characters long
fn validate_description(value: &str) -> bool {
    value.chars().count() <= 256
}

/// Checks "fields" field of operations with "schema_definition_v1" schema id.
///
/// 1. A schema must have at most 1024 fields
fn validate_fields(value: &Vec<Vec<String>>) -> bool {
    value.len() <= 1024
}

/// Validate formatting for operations following `schema_definition_v1` system schemas.
///
/// These operations contain a "name", "description" and "fields" field with each have special
/// limitations defined by the p2panda specification.
///
/// Please note that this does not check type field type or the operation fields in general, as
/// this should be handled by other validation methods. This method is only checking the
/// special requirements of this particular system schema.
pub fn validate_schema_definition_v1_fields(
    fields: &PlainFields,
) -> Result<(), SchemaDefinitionError> {
    // Check "name" field
    let schema_name = fields.get("name");

    match schema_name {
        Some(PlainValue::StringOrRelation(value)) => match SchemaName::new(value) {
            Ok(_) => Ok(()),
            Err(_) => Err(SchemaDefinitionError::NameInvalid),
        },
        _ => Ok(()),
    }?;

    // Check "description" field
    let schema_description = fields.get("description");

    match schema_description {
        Some(PlainValue::StringOrRelation(value)) => {
            if validate_description(value) {
                Ok(())
            } else {
                Err(SchemaDefinitionError::DescriptionInvalid)
            }
        }
        _ => Ok(()),
    }?;

    // Check "fields" field
    let schema_fields = fields.get("fields");

    match schema_fields {
        Some(PlainValue::PinnedRelationList(value)) => {
            if validate_fields(value) {
                Ok(())
            } else {
                Err(SchemaDefinitionError::FieldsInvalid)
            }
        }
        _ => Ok(()),
    }?;

    Ok(())
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use crate::operation::plain::{PlainFields, PlainValue};
    use crate::test_utils::constants::HASH;
    use crate::test_utils::fixtures::random_document_view_id;

    use super::{validate_description, validate_fields, validate_schema_definition_v1_fields};

    #[rstest]
    #[case(vec![
       ("name", "venues".into()),
       ("description", "This is a test description".into()),
       ("fields", vec![random_document_view_id(), random_document_view_id()].into()),
    ].into())]
    #[case::no_fields(vec![
       ("name", "venues".into()),
       ("description", "This is a test description".into()),
       ("fields", PlainValue::PinnedRelationList(Vec::new())),
    ].into())]
    #[case::no_name(vec![
       ("description", "This is a test description".into()),
       ("fields", vec![random_document_view_id()].into()),
    ].into())]
    #[case::no_description(vec![
       ("name", "venues".into()),
       ("fields", vec![random_document_view_id()].into()),
    ].into())]
    fn check_fields(#[case] fields: PlainFields) {
        assert!(validate_schema_definition_v1_fields(&fields).is_ok());
    }

    #[test]
    fn check_schema_fields() {
        let mut many_fields = Vec::new();

        for _ in 0..1200 {
            many_fields.push(vec![HASH.to_owned()]);
        }

        assert!(!validate_fields(&many_fields));
        assert!(validate_fields(&vec![vec![HASH.to_owned()]]));
    }

    #[rstest]
    #[case(
        "The kangaroo is a marsupial from the family Macropodidae
           (macropods, meaning large foot)"
    )]
    #[case("%*&______@@@@@[[}}}{}}}}}}}&}{&{&{&{&{&}}}}}]]")]
    #[should_panic]
    #[case(
        "In common use the term is used to describe the largest species from this
           family, the red kangaroo, as well as the antilopine kangaroo, eastern grey
           kangaroo, and western grey kangaroo! Kangaroos have large, powerful hind legs,
           large feet adapted for leaping, a long muscular tail for balance, and a small
           head. Like most marsupials, female kangaroos have a pouch called a marsupium
           in which joeys complete postnatal development."
    )]
    fn check_description(#[case] description_str: &str) {
        assert!(validate_description(description_str));
    }
}
