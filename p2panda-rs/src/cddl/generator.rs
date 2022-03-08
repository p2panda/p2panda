// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::schema::system::FieldType;

/// CDDL types.
#[derive(Clone, Debug)]
<<<<<<< HEAD
pub enum CddlType {
=======
pub enum Type {
>>>>>>> Complete refactor of CDDL generation code
    Bool,
    Int,
    Float,
    Tstr,
    Relation,
}

/// CDDL types to string representation.
<<<<<<< HEAD
impl CddlType {
    // Returns the CDDL type string
    fn as_str(&self) -> &str {
=======
impl Type {
    // Returns the CDDL type string
    fn as_cddl_type(&self) -> &str {
>>>>>>> Complete refactor of CDDL generation code
        match self {
            CddlType::Bool => "bool",
            CddlType::Int => "int",
            CddlType::Float => "float",
            CddlType::Tstr => "tstr",
            CddlType::Relation => "tstr .regexp \"[0-9a-f]{68}\"",
        }
    }
<<<<<<< HEAD
}

impl From<FieldType> for CddlType {
    fn from(field_type: FieldType) -> Self {
        match field_type {
            FieldType::Bool => CddlType::Bool,
            FieldType::Int => CddlType::Int,
            FieldType::Float => CddlType::Float,
            FieldType::String => CddlType::Tstr,
            FieldType::Relation => CddlType::Relation,
=======

    // Returns the p2panda `OperationValue` type string
    fn as_field_type(&self) -> &str {
        match self {
            Type::Bool => "bool",
            Type::Int => "int",
            Type::Float => "float",
            Type::Tstr => "str",
            Type::Relation => "relation",
>>>>>>> Complete refactor of CDDL generation code
        }
    }
}

<<<<<<< HEAD
<<<<<<< HEAD
type FieldName = String;

/// Generate a CDDL definition for the passed field name and type mappings.
pub fn generate_fields(fields: &BTreeMap<FieldName, FieldType>) -> String {
    let mut cddl_str = "".to_string();
    for (count, (name, field_type)) in fields.iter().enumerate() {
        if count != 0 {
            cddl_str += "\n";
        };
        cddl_str += &format!("{name} = {{ ");
        cddl_str += &format!("type: \"{}\", ", field_type.as_str());
        cddl_str += &format!(
            "value: {}, ",
            CddlType::from(field_type.to_owned()).as_str()
        );
        cddl_str += "}";
=======
/// Struct for building and representing CDDL groups.
///
/// CDDL uses groups to define reusable data structures they can be merged or used in Vectors,
/// Tables and Structs.
#[derive(Clone, Debug)]
pub struct Group(BTreeMap<String, Field>);

impl Group {
    /// Create a new CDDL group.
    pub fn new() -> Self {
        Self(BTreeMap::new())
>>>>>>> Refactor cddl_generator
=======
type FieldName = String;

// NB: These methods could accept a `DocumentView` instead of an arbitrary map. Then it would infer
// the types from the `OperationValue`.
pub fn generate_fields(fields: BTreeMap<FieldName, Type>) -> String {
    let mut cddl_str = "".to_string();
    for (count, (name, field_type)) in fields.iter().enumerate() {
        if count != 0 {
            cddl_str += "\n";
        };
        cddl_str += &format!("{name} = {{ ");
        cddl_str += &format!("type: \"{}\", ", field_type.as_field_type());
        cddl_str += &format!("value: {}, ", field_type.as_cddl_type());
        cddl_str += "}";
>>>>>>> Complete refactor of CDDL generation code
    }
    cddl_str
}

<<<<<<< HEAD
<<<<<<< HEAD
/// Generate a CDDL definition for the compulsory fields of a CREATE operation.
pub fn generate_create_fields(fields: &[&String]) -> String {
=======
pub fn generate_create_fields(fields: Vec<String>) -> String {
>>>>>>> Complete refactor of CDDL generation code
    let mut cddl_str = "create-fields = { ".to_string();
    for (count, key) in fields.iter().enumerate() {
        if count != 0 {
            cddl_str += ", ";
        }
        cddl_str += key;
<<<<<<< HEAD
=======
    /// Add a field to the group.
    pub fn add_field(&mut self, key: &str, field_type: Field) {
        self.0.insert(key.to_owned(), field_type);
>>>>>>> Refactor cddl_generator
=======
>>>>>>> Complete refactor of CDDL generation code
    }
    cddl_str += " }";
    cddl_str
}

<<<<<<< HEAD
<<<<<<< HEAD
/// Generate a CDDL definition for the optional fields of an UPDATE operation.
pub fn generate_update_fields(fields: &[&String]) -> String {
=======
pub fn generate_update_fields(fields: Vec<String>) -> String {
>>>>>>> Complete refactor of CDDL generation code
    let mut cddl_str = "update-fields = { + ( ".to_string();
    for (count, key) in fields.iter().enumerate() {
        if count != 0 {
            cddl_str += " // ";
<<<<<<< HEAD
=======
impl ToString for Group {
    fn to_string(&self) -> String {
        let mut cddl_str = "( ".to_string();
        for (count, (key, value)) in self.0.iter().enumerate() {
            // For every element except the first, add a comma
            if count != 0 {
                cddl_str += ", ";
            }
            cddl_str += &format!("{}: {}", key, value.to_string());
>>>>>>> Refactor cddl_generator
=======
>>>>>>> Complete refactor of CDDL generation code
        }
        cddl_str += key;
    }
    cddl_str += " ) }";
    cddl_str
<<<<<<< HEAD
}

<<<<<<< HEAD
/// Generate a CDDL definition according to the fields of an application schema definition.
///
/// This can be used to validate CBOR encoded operations which follow this particular application
/// schema.
pub fn generate_cddl_definition(fields: &BTreeMap<FieldName, FieldType>) -> String {
    let field_names: Vec<&String> = fields.keys().collect();
    let mut cddl_str = String::from("");

    cddl_str += &generate_fields(&fields.clone());
    cddl_str += "\n";
    cddl_str += &generate_create_fields(&field_names);
    cddl_str += "\n";
    cddl_str += &generate_update_fields(&field_names);

    cddl_str
=======
>>>>>>> Complete refactor of CDDL generation code
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
<<<<<<< HEAD

    use crate::{
        cddl::{
            generate_cddl_definition,
            generator::{generate_create_fields, generate_fields, generate_update_fields},
        },
        schema::system::FieldType,
    };

    fn person() -> BTreeMap<String, FieldType> {
        let mut person = BTreeMap::new();

        person.insert("name".to_string(), FieldType::String);
        person.insert("age".to_string(), FieldType::Int);
        person.insert("height".to_string(), FieldType::Float);
        person.insert("is_cool".to_string(), FieldType::Bool);
        person.insert("favorite_food".to_string(), FieldType::Relation);

        person
    }

    #[test]
    pub fn generate_cddl_fields() {
        let expected_fields_cddl = "age = { type: \"int\", value: int, }\n".to_string()
            + "favorite_food = { type: \"relation\", value: tstr .regexp \"[0-9a-f]{68}\", }\n"
            + "height = { type: \"float\", value: float, }\n"
            + "is_cool = { type: \"bool\", value: bool, }\n"
            + "name = { type: \"str\", value: tstr, }";
=======
/// CddlGenerator struct for programmatically creating CDDL strings.
#[derive(Clone, Debug)]
pub struct CddlGenerator(BTreeMap<String, Field>);
=======
>>>>>>> Complete refactor of CDDL generation code

    use crate::cddl::generator::{generate_create_fields, generate_update_fields};

    use super::{generate_fields, Type};

<<<<<<< HEAD
        // Create an operation field group and add fields
        let mut operation_fields = Group::new();
        operation_fields.add_field("type", Field::String(type_string.to_owned()));
        operation_fields.add_field("value", Field::Type(field_type));
>>>>>>> Refactor cddl_generator

        let fields_cddl = generate_fields(&person());

<<<<<<< HEAD
        assert_eq!(fields_cddl, expected_fields_cddl);
=======
        // Insert new operation field. If this was created from a cddl string `fields` will be None
        self.0.insert(key, operation_fields);
>>>>>>> Refactor cddl_generator
    }

<<<<<<< HEAD
    #[test]
    pub fn generate_cddl_create_fields() {
        let expected_create_fields_cddl: &str =
            "create-fields = { age, favorite_food, height, is_cool, name }";

        let person = person();
        let field_names: Vec<&String> = person.keys().collect();
        let create_fields_cddl = generate_create_fields(&field_names);

        assert_eq!(create_fields_cddl, expected_create_fields_cddl);
=======
impl ToString for CddlGenerator {
    fn to_string(&self) -> String {
        let mut cddl_str = "".to_string();
        for (count, value) in self.0.iter().enumerate() {
            if count != 0 {
                cddl_str += ", ";
            }
            cddl_str += &format!("{}: {{ {} }}", value.0, value.1.to_string());
        }
        cddl_str
>>>>>>> Refactor cddl_generator
    }

    #[test]
    pub fn generate_cddl_update_fields() {
        let expected_update_fields_cddl: &str =
            "update-fields = { + ( age // favorite_food // height // is_cool // name ) }";

        let person = person();
        let field_names: Vec<&String> = person.keys().collect();
        let update_fields_cddl = generate_update_fields(&field_names);

<<<<<<< HEAD
        assert_eq!(update_fields_cddl, expected_update_fields_cddl);
    }

    #[test]
    pub fn generates_cddl_definition() {
        let expected_cddl = "age = { type: \"int\", value: int, }\n".to_string()
            + "favorite_food = { type: \"relation\", value: tstr .regexp \"[0-9a-f]{68}\", }\n"
            + "height = { type: \"float\", value: float, }\n"
            + "is_cool = { type: \"bool\", value: bool, }\n"
            + "name = { type: \"str\", value: tstr, }\n"
            + "create-fields = { age, favorite_food, height, is_cool, name }\n"
            + "update-fields = { + ( age // favorite_food // height // is_cool // name ) }";

        let person = person();
        let generated_cddl = generate_cddl_definition(&person);

        assert_eq!(expected_cddl, generated_cddl);
=======
    pub const PERSON_CDDL: &str =
        r#"age: { ( type: "int", value: int ) }, name: { ( type: "str", value: tstr ) }"#;

    #[test]
    pub fn cddl_builder() {
        // Instantiate new empty CDDL named "person"
        let mut person = CddlGenerator::new();

        // Add two operation fields to the CDDL
        person.add_operation_field("name".to_owned(), Type::Tstr);
        person.add_operation_field("age".to_owned(), Type::Int);

        // Create a new "person" operation
        let mut me = OperationFields::new();
        me.add("name", OperationValue::Text("Sam".to_owned()))
            .unwrap();
        me.add("age", OperationValue::Integer(35)).unwrap();

        // Validate operation fields against person CDDL
        assert_eq!(person.to_string(), PERSON_CDDL);
>>>>>>> Refactor cddl_generator
=======
    fn person() -> BTreeMap<String, Type> {
        let mut person = BTreeMap::new();

        person.insert("name".to_string(), Type::Tstr);
        person.insert("age".to_string(), Type::Int);

        person
    }

    #[test]
    pub fn generate_cddl_fields() {
        let expected_fields_cddl: &str =
            "age = { type: \"int\", value: int, }\nname = { type: \"str\", value: tstr, }";

        let fields_cddl = generate_fields(person());

        assert_eq!(fields_cddl, expected_fields_cddl);
    }

    #[test]
    pub fn generate_cddl_create_fields() {
        let expected_create_fields_cddl: &str = "create-fields = { age, name }";

        let fields: Vec<String> = person().keys().cloned().collect();
        let create_fields_cddl = generate_create_fields(fields);

        assert_eq!(create_fields_cddl, expected_create_fields_cddl);
    }

    #[test]
    pub fn generate_cddl_update_fields() {
        let expected_update_fields_cddl: &str = "update-fields = { + ( age // name ) }";

        let fields: Vec<String> = person().keys().cloned().collect();
        let update_fields_cddl = generate_update_fields(fields);

        assert_eq!(update_fields_cddl, expected_update_fields_cddl);
>>>>>>> Complete refactor of CDDL generation code
    }
}
