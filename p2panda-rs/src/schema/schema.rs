// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::fmt;

use crate::hash::Hash;
use crate::operation::{Operation, OperationFields, OperationValue};
use crate::schema::SchemaError;

use cddl::lexer::Lexer;
use cddl::parser::Parser;
#[cfg(not(target_arch = "wasm32"))]
use cddl::validate_cbor_from_slice;
#[cfg(not(target_arch = "wasm32"))]
use cddl::validator::cbor;

/// CDDL types
#[derive(Clone, Debug, Copy)]
#[allow(missing_docs)]
pub enum Type {
    Bool,
    Int,
    Float,
    Tstr,
    Relation,
}

/// CDDL schema type string formats
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

/// CDDL field types
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
            Field::Struct(group) => write!(f, "{{ {} }}", format!("{}", group)),
        }
    }
}

/// Struct for building and representing CDDL groups.
/// CDDL uses groups to define reuseable data structures
/// they can be merged into schema or used in Vectors, Tables and Structs
#[derive(Clone, Debug)]
pub struct Group {
    #[allow(dead_code)] // Remove when module in use.
    name: String,
    fields: BTreeMap<String, Field>,
}

impl Group {
    /// Create a new CDDL group
    pub fn new(name: String) -> Self {
        Self {
            name,
            fields: BTreeMap::new(),
        }
    }

    /// Add an Field to the group.
    pub fn add_field(&mut self, key: &str, field_type: Field) {
        self.fields.insert(key.to_owned(), field_type);
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let map = &self.fields;
        write!(f, "( ")?;
        for (count, value) in map.iter().enumerate() {
            // For every element except the first, add a comma.
            if count != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}: {}", value.0, value.1)?;
        }
        write!(f, " )")
    }
}

/// SchemaBuilder struct for programatically creating CDDL schemas and valdating OperationFields.
#[derive(Clone, Debug)]
pub struct SchemaBuilder {
    name: String,
    fields: BTreeMap<String, Field>,
}

/// Schema struct for creating CDDL schemas, valdating OperationFields and creating operations
/// following the defined schema.
#[derive(Clone, Debug)]
pub struct Schema {
    schema_hash: Hash,
    schema_string: String,
}

impl SchemaBuilder {
    /// Create a new blank Schema
    pub fn new(name: String) -> Self {
        Self {
            name,
            fields: BTreeMap::new(),
        }
    }

    /// Add a field definition to this schema
    pub fn add_operation_field(
        &mut self,
        key: String,
        field_type: Type,
    ) -> Result<(), SchemaError> {
        // Match passed type and map it to our OperationFields type and CDDL types.
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

        // Insert new operation field into Schema fields.
        // If this Schema was created from a cddl string
        // `fields` will be None.
        self.fields.insert(key, operation_fields);
        Ok(())
    }

    /// Validate an operation against this user schema
    #[cfg(not(target_arch = "wasm32"))]
    pub fn validate_operation(&self, bytes: Vec<u8>) -> Result<(), SchemaError> {
        match validate_cbor_from_slice(&format!("{}", self), &bytes) {
            Err(cbor::Error::Validation(err)) => {
                let err = err
                    .iter()
                    .map(|fe| format!("{}: \"{}\"", fe.cbor_location, fe.reason))
                    .collect::<Vec<String>>()
                    .join(", ");

                Err(SchemaError::InvalidSchema(err))
            }
            Err(cbor::Error::CBORParsing(_err)) => Err(SchemaError::InvalidCBOR),
            Err(cbor::Error::CDDLParsing(err)) => {
                panic!("Parsing CDDL error: {}", err);
            }
            _ => Ok(()),
        }
    }
}

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
    /// Create a new Schema from a schema hash and schema CDDL string
    pub fn new(schema_hash: &Hash, schema_str: &str) -> Result<Self, SchemaError> {
        let mut lexer = Lexer::new(schema_str);
        let parser = Parser::new(lexer.iter(), schema_str);
        let schema_string = match parser {
            Ok(mut parser) => match parser.parse_cddl() {
                Ok(cddl) => Ok(cddl.to_string()),
                Err(err) => Err(SchemaError::ParsingError(err.to_string())),
            },
            Err(err) => Err(SchemaError::ParsingError(err.to_string())),
        }?;
        let schema_hash = match Hash::new(schema_hash.as_str()) {
            Ok(hash) => Ok(hash),
            Err(err) => Err(SchemaError::InvalidSchema(err.to_string())),
        }?;

        Ok(Self {
            schema_hash,
            schema_string,
        })
    }

    /// Return the hash id of this schema
    pub fn schema_hash(&self) -> Hash {
        self.schema_hash.clone()
    }

    /// Create a new CREATE operation validated against this schema
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

        match self.validate_operation(serde_cbor::to_vec(&fields.clone()).unwrap()) {
            Ok(_) => Ok(()),
            Err(err) => Err(SchemaError::ValidationError(err.to_string())),
        }?;

        match Operation::new_create(self.schema_hash(), fields) {
            Ok(hash) => Ok(hash),
            Err(err) => Err(SchemaError::OperationError(err)),
        }
    }

    /// Create a new UPDATE operation validated against this schema
    #[cfg(not(target_arch = "wasm32"))]
    pub fn update(
        &self,
        id: &str,
        key_values: Vec<(&str, &str)>,
    ) -> Result<Operation, SchemaError> {
        let mut fields = OperationFields::new();
        let id = Hash::new(id).unwrap();

        for (key, value) in key_values {
            match fields.add(key, OperationValue::Text(value.into())) {
                Ok(_) => Ok(()),
                Err(err) => Err(SchemaError::InvalidSchema(err.to_string())),
            }?;
        }

        match self.validate_operation(serde_cbor::to_vec(&fields.clone()).unwrap()) {
            Ok(_) => Ok(()),
            Err(err) => Err(SchemaError::ValidationError(err.to_string())),
        }?;

        match Operation::new_update(self.schema_hash(), id, fields) {
            Ok(hash) => Ok(hash),
            Err(err) => Err(SchemaError::InvalidSchema(err.to_string())),
        }
    }

    /// Validate an operation against this user schema
    #[cfg(not(target_arch = "wasm32"))]
    pub fn validate_operation(&self, bytes: Vec<u8>) -> Result<(), SchemaError> {
        match validate_cbor_from_slice(&format!("{}", self), &bytes) {
            Err(cbor::Error::Validation(err)) => {
                let err = err
                    .iter()
                    .map(|fe| format!("{}: \"{}\"", fe.cbor_location, fe.reason))
                    .collect::<Vec<String>>()
                    .join(", ");

                Err(SchemaError::InvalidSchema(err))
            }
            Err(cbor::Error::CBORParsing(_err)) => Err(SchemaError::InvalidCBOR),
            Err(cbor::Error::CDDLParsing(err)) => {
                panic!("Parsing CDDL error: {}", err);
            }
            _ => Ok(()),
        }
    }
}

impl fmt::Display for Schema {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.schema_string)
    }
}

#[cfg(test)]
mod tests {
    use crate::hash::Hash;
    use crate::operation::{Operation, OperationFields, OperationValue};
    use crate::schema::{Schema, SchemaBuilder, Type};

    /// All user schema
    pub const USER_SCHEMA: &str = r#"
    userSchema = {
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

    /// All user schema hash
    pub const USER_SCHEMA_HASH: &str =
        "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543";

    /// Person schema
    pub const PERSON_SCHEMA: &str = r#"
    person = (
        name: { type: "str", value: tstr },
        age: { type: "int", value: int },
    )
    "#;

    /// Person schema hash
    pub const PERSON_SCHEMA_HASH: &str =
        "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543";

    #[test]
    pub fn schema_builder() {
        // Instanciate new empty schema named "person"
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
        let me_bytes = serde_cbor::to_vec(&me).unwrap();
        assert!(person.validate_operation(me_bytes).is_ok());
    }

    #[test]
    pub fn schema_from_string() {
        // Create a new "person" operation
        let mut me = OperationFields::new();
        me.add("name", OperationValue::Text("Sam".to_owned()))
            .unwrap();
        me.add("age", OperationValue::Integer(35)).unwrap();

        // Instanciate "person" schema from cddl string
        let cddl_str = "person = { (
            age: { type: \"int\", value: int },
            name: { type: \"str\", value: tstr }
        ) }";

        let person_from_string =
            Schema::new(&Hash::new(USER_SCHEMA_HASH).unwrap(), &cddl_str.to_string()).unwrap();

        // Validate operation fields against person schema
        let me_bytes = serde_cbor::to_vec(&me).unwrap();
        assert!(person_from_string.validate_operation(me_bytes).is_ok());
    }

    #[test]
    pub fn validate_against_megaschema() {
        // Instanciate global user schema from mega schema string and it's hash
        let user_schema = Schema::new(
            &Hash::new(USER_SCHEMA_HASH).unwrap(),
            &USER_SCHEMA.to_string(),
        )
        .unwrap();

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

        // Validate operation fields against user schema
        let me_bytes = serde_cbor::to_vec(&me).unwrap();
        let my_address_bytes = serde_cbor::to_vec(&my_address).unwrap();

        assert!(user_schema.validate_operation(me_bytes).is_ok());
        assert!(user_schema.validate_operation(my_address_bytes).is_ok());

        // Operations not matching one of the user schema should fail
        let mut naughty_panda = OperationFields::new();
        naughty_panda
            .add("name", OperationValue::Text("Naughty Panda".to_owned()))
            .unwrap();
        naughty_panda
            .add("colour", OperationValue::Text("pink & orange".to_owned()))
            .unwrap();

        let naughty_panda_bytes = serde_cbor::to_vec(&naughty_panda).unwrap();
        assert!(user_schema.validate_operation(naughty_panda_bytes).is_err());
    }

    #[test]
    pub fn create_operation() {
        let person_schema = Schema::new(
            &Hash::new(PERSON_SCHEMA_HASH).unwrap(),
            &PERSON_SCHEMA.to_string(),
        )
        .unwrap();

        // Create an operation the long way without validation
        let mut operation_fields = OperationFields::new();
        operation_fields
            .add("name", OperationValue::Text("Panda".to_owned()))
            .unwrap();
        operation_fields
            .add("age", OperationValue::Integer(12))
            .unwrap();

        let operation =
            Operation::new_create(Hash::new(PERSON_SCHEMA_HASH).unwrap(), operation_fields)
                .unwrap();

        // Create an operation the quick way *with* validation
        let operation_again = person_schema
            .create(vec![
                ("name", OperationValue::Text("Panda".to_string())),
                ("age", OperationValue::Integer(12)),
            ])
            .unwrap();

        assert_eq!(operation, operation_again);
    }
}
