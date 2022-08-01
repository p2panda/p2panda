// SPDX-License-Identifier: AGPL-&3.0-or-later

use std::str::FromStr;

use lazy_static::lazy_static;
use regex::Regex;

use crate::next::operation::plain::{PlainFields, PlainValue};
use crate::next::schema::validate::error::SchemaFieldDefinitionError;
use crate::next::schema::SchemaId;

/// Checks "name" field in a schema field definition operation.
///
/// 1. It must be at most 64 characters long
/// 2. It begins with a letter
/// 3. It uses only alphanumeric characters, digits and the underscore character
fn validate_name(value: &str) -> bool {
    lazy_static! {
        // Unwrap as we checked the regular expression for correctness
        static ref NAME_REGEX: Regex = Regex::new("^[A-Za-z]{1}[A-Za-z0-9_]{0,63}$").unwrap();
    }

    NAME_REGEX.is_match(value)
}

/// Checks "type" field in a schema field definition operation.
///
/// 1. It must be one of: bool, int, float, str, relation, pinned_relation, relation_list,
///    pinned_relation_list
/// 2. Relations need to specify a valid and canonical schema id
fn validate_type(value: &str) -> bool {
    match value {
        "bool" | "int" | "float" | "str" => true,
        relation => validate_relation_type(relation),
    }
}

/// Checks format for "type" fields which specify a relation.
///
/// 1. The first section is the name, which must have 1-64 characters, must start with a letter and
///    must contain only alphanumeric characters and underscores
/// 2. The remaining sections are the document view id, represented as alphabetically sorted
///    hex-encoded operation ids, separated by underscores
fn validate_relation_type(value: &str) -> bool {
    // Parse relation value
    lazy_static! {
        static ref RELATION_REGEX: Regex = {
            let schema_id = "[A-Za-z]{1}[A-Za-z0-9_]{0,63}_([0-9A-Za-z]{68})(_[0-9A-Za-z]{68})*";

            // Unwrap as we checked the regular expression for correctness
            Regex::new(&format!(r"(\w+)\(({})*\)", schema_id)).unwrap()
        };
    }

    let groups = RELATION_REGEX.captures(value);
    if groups.is_none() {
        return false;
    }

    let relation_type_str = groups
        .as_ref()
        // Unwrap now as we checked if its `None` before
        .unwrap()
        .get(1)
        .map(|group_match| group_match.as_str());

    let schema_id_str = groups
        .as_ref()
        // Unwrap now as we checked if its `None` before
        .unwrap()
        .get(2)
        .map(|group_match| group_match.as_str());

    // Check if relation type is known
    let is_valid_relation_type = match relation_type_str {
        Some(type_str) => {
            matches!(
                type_str,
                "relation" | "pinned_relation" | "relation_list" | "pinned_relation_list"
            )
        }
        None => false,
    };

    // Check if schema id is correctly (valid hashes) and canonically formatted (no duplicates,
    // sorted operation ids)
    let is_valid_schema_id = match schema_id_str {
        Some(str) => {
            return SchemaId::from_str(str).is_ok();
        }
        None => false,
    };

    is_valid_relation_type && is_valid_schema_id
}

/// Validate formatting for operations following `schema_field_definition_v1` system schemas.
///
/// These operations contain a "name" and "type" field with each have special limitations defined
/// by the p2panda specification.
pub fn validate_schema_field_definition_v1_fields(
    fields: &PlainFields,
) -> Result<(), SchemaFieldDefinitionError> {
    // Check that there are only two fields given
    if fields.len() != 2 {
        return Err(SchemaFieldDefinitionError::UnexpectedFields);
    }

    // Check "name" field
    let field_name = fields
        .get("name")
        .ok_or(SchemaFieldDefinitionError::NameMissing)?;

    match field_name {
        PlainValue::StringOrRelation(value) => {
            if validate_name(value) {
                Ok(())
            } else {
                Err(SchemaFieldDefinitionError::NameInvalid)
            }
        }
        _ => Err(SchemaFieldDefinitionError::NameWrongType),
    }?;

    // Check "type" field
    let field_type = fields
        .get("type")
        .ok_or(SchemaFieldDefinitionError::TypeMissing)?;

    match field_type {
        PlainValue::StringOrRelation(value) => {
            if validate_type(value) {
                Ok(())
            } else {
                Err(SchemaFieldDefinitionError::TypeInvalid)
            }
        }
        _ => Err(SchemaFieldDefinitionError::TypeWrongType),
    }?;

    Ok(())
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use crate::next::operation::plain::PlainFields;

    use super::{validate_name, validate_schema_field_definition_v1_fields, validate_type};

    #[rstest]
    #[case(vec![
       ("name", "goodPlacesForChoirRehearsal".into()),
       ("type", "str".into()),
    ].into())]
    #[case(vec![
       ("name", "a__is___".into()),
       ("type", "bool".into()),
    ].into())]
    #[should_panic]
    #[case::missing_type(vec![("name", "venue".into())].into())]
    #[should_panic]
    #[case::missing_name(vec![("type", "str".into())].into())]
    #[should_panic]
    #[case::unknown_field(vec![
      ("name", "venue".into()),
      ("type", "str".into()),
      ("clean", "ing".into()),
    ].into())]
    #[should_panic]
    #[case::invalid_type(vec![
      ("name", "venue".into()),
      ("type", "string".into()),
    ].into())]
    #[should_panic]
    #[case::invalid_name(vec![
      ("name", "venuüüüü".into()),
      ("type", "str".into()),
    ].into())]
    fn check_fields(#[case] fields: PlainFields) {
        assert!(validate_schema_field_definition_v1_fields(&fields.into()).is_ok());
    }

    #[rstest]
    #[case("venues_with_garden")]
    #[case("animals_in_zoo_with_many_friends")]
    #[case("robot_3000_building_specification")]
    #[case("mushrooms_in_2054")]
    #[case("ILikeCamels")]
    #[case("AndDromedars")]
    #[case("And_Their_Special_Variants")]
    #[case("where_did_we_end_up_again_")]
    #[case("c0_1_2_1_a_b_4_____")]
    #[should_panic]
    #[case("")]
    #[should_panic]
    #[case("venüë")]
    #[should_panic]
    #[case("サービス！サービス！")]
    #[should_panic]
    #[case("schema_field_names_for_people_who_cant_decide_which_schema_field_name_to_pick")]
    #[should_panic]
    #[case("25_kangaroos")]
    #[should_panic]
    #[case("_and_how_did_it_all_began")]
    #[should_panic]
    #[case("?")]
    #[should_panic]
    #[case("specification-says-no")]
    fn check_name_field(#[case] name_str: &str) {
        assert!(validate_name(name_str));
    }

    #[rstest]
    #[case("bool")]
    #[case("int")]
    #[case("str")]
    #[case("float")]
    #[case(concat!(
        "relation(",
        "venues_with_garden",
        "_0020f63666b2f7d629136e163004afcf6782473637357f36c2e90b6ab2ca9a977531)"
    ))]
    #[case(concat!(
        "pinned_relation(",
        "monkeys",
        "_0020f63666b2f7d629136e163004afcf6782473637357f36c2e90b6ab2ca9a977531)"
    ))]
    #[case(concat!(
        "relation_list(",
        "bees",
        "_0020506d20110d41bfcf6ee0b8c49d43add6d97e1ef266f693b91023393f2dc4a4b9",
        "_0020f9ccd520ee0fe7c2f5ff8d878b7d2f5b4edf38b3eff9777e5ea49bc3c467dfdf",
        "_0020ff592c9bd526fcf129f5bece2ef2429b07a15ba09739194628ae443977766a56)"
    ))]
    #[case(concat!(
        "pinned_relation_list(",
        "and_recommendations",
        "_0020087be825aea1779ea192860671abfa5c6ac4b7d990156a2e0d3ed051816f128b",
        "_0020f63666b2f7d629136e163004afcf6782473637357f36c2e90b6ab2ca9a977531)"
    ))]
    #[should_panic]
    #[case("")]
    #[should_panic]
    #[case("floaty")]
    #[should_panic]
    #[case("boaty")]
    #[should_panic]
    #[case(concat!(
        "relation(inny!_boxy!_dynny!_thingy!",
        "_0020bf46222486048a22dc6298f7257ae65885d15a3421ad391969824b393cba8ad3)"
    ))]
    #[should_panic]
    #[case("pinned_relation(his_is_not_a_hash)")]
    #[should_panic]
    #[case("relation_list(enues_00201234)")]
    #[should_panic]
    #[case(concat!(
        "pinned_relation_list(",
        "unordered_operation_ids",
        "_0020b685e05fe70a215db1d45b5ae3de60f1ce0d72e7c33cf4a25792ba21a6f960b6",
        "_00207b69a78ab4fb53060f55e2eff6da3d8fb78df753e8ebce605fae250b4214179f)"
    ))]
    #[should_panic]
    #[case(concat!(
        "relation(",
        "duplicate_operation_ids",
        "_002018731a680a9cb1849ded94441c06546238a30842f69af3b1879b8b31f0312b38",
        "_002018731a680a9cb1849ded94441c06546238a30842f69af3b1879b8b31f0312b38)"
    ))]
    fn check_type_field(#[case] type_str: &str) {
        assert!(validate_type(type_str));
    }
}
