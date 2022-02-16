// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::convert::TryFrom;
use std::fmt;

use cddl::lexer::Lexer;
use cddl::parser::Parser;
#[cfg(not(target_arch = "wasm32"))]
use cddl::validate_cbor_from_slice;
#[cfg(not(target_arch = "wasm32"))]
use cddl::validator::cbor;

#[cfg(not(target_arch = "wasm32"))]
use crate::document::{DocumentView, DocumentViewError};
use crate::hash::Hash;
#[cfg(not(target_arch = "wasm32"))]
use crate::operation::{AsOperation, Operation, OperationFields, OperationValue};
use crate::schema::SchemaError;

use super::SchemaType;

/// CDDL types.
#[derive(Clone, Debug, Copy)]
#[allow(missing_docs)]
pub enum Type {
    Bool,
    Int,
    Float,
    Tstr,
    Relation,
}

/// CDDL schema type string formats.
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let cddl_type = match self {
            Type::Bool => "bool",
            Type::Int => "int",
            Type::Float => "float",
            Type::Tstr => "tstr",
            Type::Relation => "tstr .regexp \"[0-9a-f]{68}\"",
        };
        write!(f, "{}", cddl_type)
    }
}

/// CDDL field types.
#[derive(Clone, Debug)]
pub enum Field {
    String(String),
    Type(Type),
    Struct(Group),
}

/// Format each different data type into a schema string.
impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Field::String(s) => write!(f, "\"{}\"", s),
            Field::Type(cddl_type) => write!(f, "{}", cddl_type),
            Field::Struct(group) => write!(f, "{{ {} }}", group),
        }
    }
}

/// Struct for building and representing CDDL groups. CDDL uses groups to define reusable data
/// structures they can be merged into schema or used in Vectors, Tables and Structs.
#[derive(Clone, Debug)]
pub struct Group {
    #[allow(dead_code)] // Remove when schema module is used.
    name: String,
    fields: BTreeMap<String, Field>,
}

impl Group {
    /// Create a new CDDL group.
    pub fn new(name: String) -> Self {
        Self {
            name,
            fields: BTreeMap::new(),
        }
    }

    /// Add a field to the group.
    pub fn add_field(&mut self, key: &str, field_type: Field) {
        self.fields.insert(key.to_owned(), field_type);
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let map = &self.fields;
        write!(f, "( ")?;
        for (count, value) in map.iter().enumerate() {
            // For every element except the first, add a comma
            if count != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}: {}", value.0, value.1)?;
        }
        write!(f, " )")
    }
}

/// SchemaBuilder struct for programmatically creating CDDL schemas and validating OperationFields.
#[derive(Clone, Debug)]
pub struct SchemaBuilder {
    name: String,
    fields: BTreeMap<String, Field>,
}

/// Schema struct for creating CDDL schemas, validating OperationFields and creating operations
/// following the defined schema.
#[derive(Clone, Debug)]
pub struct Schema {
    schema: SchemaType,
    schema_string: String,
}

impl SchemaBuilder {
    /// Create a new blank `Schema`.
    pub fn new(name: String) -> Self {
        Self {
            name,
            fields: BTreeMap::new(),
        }
    }

    /// Add a field definition to this schema.
    pub fn add_operation_field(
        &mut self,
        key: String,
        field_type: Type,
    ) -> Result<(), SchemaError> {
        // Match passed type and map it to our OperationFields type and CDDL types
        let type_string = match field_type {
            Type::Tstr => "str",
            Type::Int => "int",
            Type::Float => "float",
            Type::Bool => "bool",
            Type::Relation => "relation",
        };

        // Create an operation field group and add fields
        let mut operation_fields = Group::new(key.to_owned());
        operation_fields.add_field("type", Field::String(type_string.to_owned()));
        operation_fields.add_field("value", Field::Type(field_type));

        // Format operation fields group as a struct
        let operation_fields = Field::Struct(operation_fields);

        // Insert new operation field into Schema fields. If this Schema was created from a cddl
        // string `fields` will be None
        self.fields.insert(key, operation_fields);
        Ok(())
    }
}

impl ValidateOperation for SchemaBuilder {}

impl fmt::Display for SchemaBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} = {{ ", self.name)?;
        for (count, value) in self.fields.iter().enumerate() {
            if count != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}: {}", value.0, value.1)?;
        }
        writeln!(f, " }}")
    }
}

impl Schema {
    /// Create a new Schema from a schema hash and schema CDDL string.
    pub fn new(schema: &SchemaType, schema_str: &str) -> Result<Self, SchemaError> {
        let mut lexer = Lexer::new(schema_str);
        let parser = Parser::new(lexer.iter(), schema_str);

        let schema_string = match parser {
            Ok(mut parser) => match parser.parse_cddl() {
                Ok(cddl) => Ok(cddl.to_string()),
                Err(err) => Err(SchemaError::ParsingError(err.to_string())),
            },
            Err(err) => Err(SchemaError::ParsingError(err.to_string())),
        }?;

        Ok(Self {
            schema: schema.to_owned(),
            schema_string,
        })
    }

    /// Return the hash id of this schema.
    pub fn schema(&self) -> SchemaType {
        self.schema.clone()
    }

    /// Create a new CREATE operation validated against this schema.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn create(
        &self,
        key_values: Vec<(&str, OperationValue)>,
    ) -> Result<Operation, SchemaError> {
        let mut fields = OperationFields::new();

        for (key, value) in key_values {
            match fields.add(key, value) {
                Ok(_) => Ok(()),
                Err(err) => Err(SchemaError::OperationFieldsError(err)),
            }?;
        }

        match self.validate_operation_fields(&fields.clone()) {
            Ok(_) => Ok(()),
            Err(err) => Err(SchemaError::ValidationError(err.to_string())),
        }?;

        match Operation::new_create(self.schema(), fields) {
            Ok(hash) => Ok(hash),
            Err(err) => Err(SchemaError::OperationError(err)),
        }
    }

    /// Create a new UPDATE operation validated against this schema.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn update(
        &self,
        previous_operations: Vec<Hash>,
        key_values: Vec<(&str, OperationValue)>,
    ) -> Result<Operation, SchemaError> {
        let mut fields = OperationFields::new();

        for (key, value) in key_values {
            match fields.add(key, value) {
                Ok(_) => Ok(()),
                Err(err) => Err(SchemaError::InvalidSchema(vec![err.to_string()])),
            }?;
        }

        match self.validate_operation_fields(&fields.clone()) {
            Ok(_) => Ok(()),
            Err(err) => Err(SchemaError::ValidationError(err.to_string())),
        }?;

        match Operation::new_update(self.schema(), previous_operations, fields) {
            Ok(hash) => Ok(hash),
            Err(err) => Err(SchemaError::InvalidSchema(vec![err.to_string()])),
        }
    }

    /// Returns a new `DocumentView` converted from CREATE `Operation` and validated against it's
    /// schema definition.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn instance_from_create(
        &self,
        operation: Operation,
    ) -> Result<DocumentView, DocumentViewError> {
        match self.validate_operation_fields(&operation.fields().unwrap()) {
            Ok(_) => Ok(()),
            Err(err) => Err(DocumentViewError::ValidationError(err)),
        }?;

        DocumentView::try_from(operation)
    }
}

impl fmt::Display for Schema {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.schema_string)
    }
}

/// Validate an operations fields against this schema
pub trait ValidateOperation
where
    Self: fmt::Display,
{
    /// Validate an operation against this application schema.
    #[cfg(not(target_arch = "wasm32"))]
    fn validate_operation_fields(
        &self,
        operation_fields: &OperationFields,
    ) -> Result<(), SchemaError> {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&operation_fields.clone(), &mut cbor_bytes).unwrap();

        match validate_cbor_from_slice(&format!("{}", self), &cbor_bytes) {
            Err(cbor::Error::Validation(err)) => {
                let err = err
                    .iter()
                    .map(|fe| format!("{}: \"{}\"", fe.cbor_location, fe.reason))
                    .collect::<Vec<String>>()
                    .join(", ");

                Err(SchemaError::InvalidSchema(vec![err]))
            }
            Err(cbor::Error::CBORParsing(_err)) => Err(SchemaError::InvalidCBOR),
            Err(cbor::Error::CDDLParsing(err)) => {
                panic!("Parsing CDDL error: {}", err);
            }
            _ => Ok(()),
        }
    }
}

impl ValidateOperation for Schema {}

// @TODO: This currently makes sure the wasm tests work as cddl does not have any wasm support
// (yet). Remove this with: https://github.com/p2panda/p2panda/issues/99
#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::hash::Hash;
    use crate::operation::{Operation, OperationFields, OperationValue};
    use crate::schema::SchemaType;
    use crate::test_utils::fixtures::{create_operation, schema};

    use super::{Schema, SchemaBuilder, Type, ValidateOperation};

    // Complete application schema.
    pub const APPLICATION_SCHEMA: &str = r#"
        applicationSchema = {
            address //
            person
        }

        address = (
            city: { type: "str", value: tstr },
            street: { type: "str", value: tstr },
            house-number: { type: "int", value: int },
        )

        person = (
            name: { type: "str", value: tstr },
            age: { type: "int", value: int },
        )
    "#;

    // Only `person` schema.
    pub const PERSON_SCHEMA: &str = r#"
        person = (
            name: { type: "str", value: tstr },
            age: { type: "int", value: int },
        )
    "#;

    #[test]
    pub fn schema_builder() {
        // Instantiate new empty schema named "person"
        let mut person = SchemaBuilder::new("person".to_owned());

        // Add two operation fields to the schema
        person
            .add_operation_field("name".to_owned(), Type::Tstr)
            .unwrap();
        person
            .add_operation_field("age".to_owned(), Type::Int)
            .unwrap();

        // Create a new "person" operation
        let mut me = OperationFields::new();
        me.add("name", OperationValue::Text("Sam".to_owned()))
            .unwrap();
        me.add("age", OperationValue::Integer(35)).unwrap();

        // Validate operation fields against person schema
        assert!(person.validate_operation_fields(&me).is_ok());
    }

    #[rstest]
    pub fn schema_from_string(schema: SchemaType) {
        // Create a new "person" operation
        let mut me = OperationFields::new();
        me.add("name", OperationValue::Text("Sam".to_owned()))
            .unwrap();
        me.add("age", OperationValue::Integer(35)).unwrap();

        // Instantiate "person" schema from cddl string
        let cddl_str = "person = { (
            age: { type: \"int\", value: int },
            name: { type: \"str\", value: tstr }
        ) }";

        let person_from_string = Schema::new(&schema, &cddl_str.to_string()).unwrap();

        // Validate operation fields against person schema
        assert!(person_from_string.validate_operation_fields(&me).is_ok());
    }

    #[rstest]
    pub fn validate_against_megaschema(schema: SchemaType) {
        // Instantiate global application schema from mega schema string and it's hash
        let application_schema = Schema::new(&schema, &APPLICATION_SCHEMA.to_string()).unwrap();

        let mut me = OperationFields::new();
        me.add("name", OperationValue::Text("Sam".to_owned()))
            .unwrap();
        me.add("age", OperationValue::Integer(35)).unwrap();

        let mut my_address = OperationFields::new();
        my_address
            .add("house-number", OperationValue::Integer(8))
            .unwrap();
        my_address
            .add("street", OperationValue::Text("Panda Lane".to_owned()))
            .unwrap();
        my_address
            .add("city", OperationValue::Text("Bamboo Town".to_owned()))
            .unwrap();

        // Validate operation fields against application schema
        assert!(application_schema.validate_operation_fields(&me).is_ok());
        assert!(application_schema
            .validate_operation_fields(&my_address)
            .is_ok());

        // Operations not matching one of the application schema should fail
        let mut naughty_panda = OperationFields::new();
        naughty_panda
            .add("name", OperationValue::Text("Naughty Panda".to_owned()))
            .unwrap();
        naughty_panda
            .add("colour", OperationValue::Text("pink & orange".to_owned()))
            .unwrap();

        assert!(application_schema
            .validate_operation_fields(&naughty_panda)
            .is_err());
    }

    #[rstest]
    pub fn test_create_operation(schema: SchemaType) {
        let person_schema = Schema::new(&schema, &PERSON_SCHEMA.to_string()).unwrap();

        // Create an operation the long way without validation
        let mut operation_fields = OperationFields::new();
        operation_fields
            .add("name", OperationValue::Text("Panda".to_owned()))
            .unwrap();
        operation_fields
            .add("age", OperationValue::Integer(12))
            .unwrap();

        let operation = Operation::new_create(schema, operation_fields).unwrap();

        // Create an operation the quick way *with* validation
        let operation_again = person_schema
            .create(vec![
                ("name", OperationValue::Text("Panda".to_string())),
                ("age", OperationValue::Integer(12)),
            ])
            .unwrap();

        assert_eq!(operation, operation_again);
    }

    #[rstest]
    pub fn test_update_operation(schema: SchemaType) {
        let person_schema = Schema::new(&schema, &PERSON_SCHEMA.to_string()).unwrap();

        // Create a operation the long way without validation
        let mut operation_fields = OperationFields::new();
        operation_fields
            .add("name", OperationValue::Text("Panda".to_owned()))
            .unwrap();
        operation_fields
            .add("age", OperationValue::Integer(12))
            .unwrap();

        let operation = Operation::new_update(
            schema,
            vec![Hash::new_from_bytes(vec![12, 128]).unwrap()],
            operation_fields,
        )
        .unwrap();

        // Create an operation the quick way *with* validation
        let operation_again = person_schema
            .update(
                vec![Hash::new_from_bytes(vec![12, 128]).unwrap()],
                vec![
                    ("name", OperationValue::Text("Panda".to_string())),
                    ("age", OperationValue::Integer(12)),
                ],
            )
            .unwrap();

        assert_eq!(operation, operation_again);
    }

    #[rstest]
    pub fn create_validate_instance(schema: SchemaType, create_operation: Operation) {
        // Instantiate "person" schema from cddl string
        let chat_schema_defnition = "chat = { (
                    message: { type: \"str\", value: tstr }
                ) }";

        let chat = Schema::new(&schema, &chat_schema_defnition.to_string()).unwrap();

        let chat_instance = chat.instance_from_create(create_operation.clone());

        assert!(chat_instance.is_ok());

        let not_chat_schema_defnition = "chat = { (
            number: { type: \"int\", value: int }
        ) }";

        let number = Schema::new(&schema, &not_chat_schema_defnition.to_string()).unwrap();

        let not_chat_instance = number.instance_from_create(create_operation);

        assert!(not_chat_instance.is_err())
    }
}
