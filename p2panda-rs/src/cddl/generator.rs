// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::schema::FieldType;

/// CDDL types.
#[derive(Clone, Debug)]
pub enum CddlType {
    Bool,
    Int,
    Float,
    Tstr,
    Relation,
    RelationList,
    PinnedRelation,
    PinnedRelationList,
}

/// CDDL types to string representation.
impl CddlType {
    // Returns the CDDL type string
    fn as_str(&self) -> &str {
        match self {
            CddlType::Bool => "bool",
            CddlType::Int => "int",
            CddlType::Float => "float",
            CddlType::Tstr => "tstr",
            CddlType::Relation => "tstr .regexp \"[0-9a-f]{68}\"",
            CddlType::RelationList => "[* tstr .regexp \"[0-9a-f]{68}\"]",
            CddlType::PinnedRelation => "[+ tstr .regexp \"[0-9a-f]{68}\"]",
            CddlType::PinnedRelationList => "[* [+ tstr .regexp \"[0-9a-f]{68}\"]]",
        }
    }
}

impl From<FieldType> for CddlType {
    fn from(field_type: FieldType) -> Self {
        match field_type {
            FieldType::Bool => CddlType::Bool,
            FieldType::Int => CddlType::Int,
            FieldType::Float => CddlType::Float,
            FieldType::String => CddlType::Tstr,
            FieldType::Relation(_) => CddlType::Relation,
            FieldType::RelationList(_) => CddlType::RelationList,
            FieldType::PinnedRelation(_) => CddlType::PinnedRelation,
            FieldType::PinnedRelationList(_) => CddlType::PinnedRelationList,
        }
    }
}

type FieldName = String;

/// Generate a CDDL definition for the passed field name and type mappings.
pub fn generate_fields(fields: &BTreeMap<FieldName, FieldType>) -> String {
    let mut cddl_str = String::new();

    for (count, (name, field_type)) in fields.iter().enumerate() {
        if count != 0 {
            let _ = writeln!(cddl_str);
        };

        let _ = write!(
            cddl_str,
            "{} = {{ type: \"{}\", value: {}, }}",
            name,
            field_type.serialise(),
            CddlType::from(field_type.to_owned()).as_str()
        );
    }

    cddl_str
}

/// Generate a CDDL definition for the compulsory fields of a CREATE operation.
pub fn generate_create_fields(fields: &[&String]) -> String {
    let mut cddl_str = "create-fields = { ".to_string();

    for (count, key) in fields.iter().enumerate() {
        if count != 0 {
            let _ = write!(cddl_str, ", ");
        }
        let _ = write!(cddl_str, "{}", key);
    }

    let _ = write!(cddl_str, " }}");

    cddl_str
}

/// Generate a CDDL definition for the optional fields of an UPDATE operation.
pub fn generate_update_fields(fields: &[&String]) -> String {
    let mut cddl_str = "update-fields = { + ( ".to_string();

    for (count, key) in fields.iter().enumerate() {
        if count != 0 {
            let _ = write!(cddl_str, " // ");
        }
        let _ = write!(cddl_str, "{}", key);
    }

    let _ = write!(cddl_str, " ) }}");

    cddl_str
}

/// Generate a CDDL definition according to the fields of an application schema definition.
///
/// This can be used to validate CBOR encoded operations which follow this particular application
/// schema.
pub fn generate_cddl_definition(fields: &BTreeMap<FieldName, FieldType>) -> String {
    let field_names: Vec<&String> = fields.keys().collect();

    format!(
        "{}\n{}\n{}",
        &generate_fields(&fields.clone()),
        &generate_create_fields(&field_names),
        &generate_update_fields(&field_names),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::cddl::generate_cddl_definition;
    use crate::cddl::generator::{generate_create_fields, generate_fields, generate_update_fields};
    use crate::schema::{FieldType, SchemaId};
    use crate::test_utils::constants::SCHEMA_ID;

    fn person() -> BTreeMap<String, FieldType> {
        let mut person = BTreeMap::new();
        let test_schema: SchemaId = SCHEMA_ID.parse().unwrap();

        person.insert("name".to_string(), FieldType::String);
        person.insert("age".to_string(), FieldType::Int);
        person.insert("height".to_string(), FieldType::Float);
        person.insert("is_cool".to_string(), FieldType::Bool);
        person.insert(
            "favorite_food".to_string(),
            FieldType::Relation(test_schema.clone()),
        );
        person.insert(
            "top_ten_foods".to_string(),
            FieldType::RelationList(test_schema.clone()),
        );
        person.insert(
            "one_specific_meal".to_string(),
            FieldType::PinnedRelation(test_schema.clone()),
        );
        person.insert(
            "top_ten_specific_meals".to_string(),
            FieldType::PinnedRelationList(test_schema),
        );

        person
    }

    #[test]
    pub fn generate_cddl_fields() {
        let expected_fields_cddl = "age = { type: \"int\", value: int, }\n\
           favorite_food = { type: \"relation(venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b)\", value: tstr .regexp \"[0-9a-f]{68}\", }\n\
           height = { type: \"float\", value: float, }\n\
           is_cool = { type: \"bool\", value: bool, }\n\
           name = { type: \"str\", value: tstr, }\n\
           one_specific_meal = { type: \"pinned_relation(venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b)\", value: [+ tstr .regexp \"[0-9a-f]{68}\"], }\n\
           top_ten_foods = { type: \"relation_list(venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b)\", value: [* tstr .regexp \"[0-9a-f]{68}\"], }\n\
           top_ten_specific_meals = { type: \"pinned_relation_list(venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b)\", value: [* [+ tstr .regexp \"[0-9a-f]{68}\"]], }";

        let fields_cddl = generate_fields(&person());

        assert_eq!(fields_cddl, expected_fields_cddl);
    }

    #[test]
    pub fn generate_cddl_create_fields() {
        let expected_create_fields_cddl = "create-fields = { age, \
            favorite_food, height, is_cool, name, one_specific_meal, \
            top_ten_foods, top_ten_specific_meals }";

        let person = person();
        let field_names: Vec<&String> = person.keys().collect();
        let create_fields_cddl = generate_create_fields(&field_names);

        assert_eq!(create_fields_cddl, expected_create_fields_cddl);
    }

    #[test]
    pub fn generate_cddl_update_fields() {
        let expected_update_fields_cddl = "update-fields = { + ( \
            age // favorite_food // height // is_cool // name // \
            one_specific_meal // top_ten_foods // \
            top_ten_specific_meals ) }";

        let person = person();
        let field_names: Vec<&String> = person.keys().collect();
        let update_fields_cddl = generate_update_fields(&field_names);

        assert_eq!(update_fields_cddl, expected_update_fields_cddl);
    }

    #[test]
    pub fn generates_cddl_definition() {
        let expected_cddl = "age = { type: \"int\", value: int, }\n\
           favorite_food = { type: \"relation(venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b)\", value: tstr .regexp \"[0-9a-f]{68}\", }\n\
           height = { type: \"float\", value: float, }\n\
           is_cool = { type: \"bool\", value: bool, }\n\
           name = { type: \"str\", value: tstr, }\n\
           one_specific_meal = { type: \"pinned_relation(venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b)\", value: [+ tstr .regexp \"[0-9a-f]{68}\"], }\n\
           top_ten_foods = { type: \"relation_list(venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b)\", value: [* tstr .regexp \"[0-9a-f]{68}\"], }\n\
           top_ten_specific_meals = { type: \"pinned_relation_list(venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b)\", value: [* [+ tstr .regexp \"[0-9a-f]{68}\"]], }\n\
           create-fields = { age, favorite_food, height, is_cool, name, one_specific_meal, top_ten_foods, top_ten_specific_meals }\n\
           update-fields = { + ( age // favorite_food // height // is_cool // name // one_specific_meal // top_ten_foods // top_ten_specific_meals ) }";

        let person = person();
        let generated_cddl = generate_cddl_definition(&person);

        assert_eq!(expected_cddl, generated_cddl);
    }
}
