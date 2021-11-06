use std::collections::BTreeMap;
use std::fmt;

use crate::hash::Hash;
use crate::message::{Message, MessageFields, MessageValue};
use cddl::lexer::Lexer;
use cddl::parser::Parser;
#[cfg(not(target_arch = "wasm32"))]
use cddl::validate_cbor_from_slice;
#[cfg(not(target_arch = "wasm32"))]
use cddl::validator::cbor;

use thiserror::Error;

/// Custom error types for schema validation.
#[derive(Error, Debug)]
pub enum SchemaError {
    /// Message contains invalid fields.
    #[error("invalid message schema: {0}")]
    InvalidSchema(String),

    /// Message can't be deserialized from invalid CBOR encoding.
    #[error("invalid CBOR format")]
    InvalidCBOR,

    /// There is no schema set
    #[error("no CDDL schema present")]
    NoSchema,

    /// Error while parsing CDDL
    #[error("error while parsing CDDL: {0}")]
    ParsingError(String),

    /// Message validation error
    #[error("invalid message values")]
    ValidationError(String),
}

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
            Type::Relation => "tstr .regexp \"[0-9a-fa-f]{132}\"",
        };
        write!(f, "{}", cddl_type)
    }
}

/// CDDL field types
#[derive(Clone, Debug)]
pub enum Field {
    String(String),
    Type(Type),
    Group(Group),
    Vector(Group),
    Struct(Group),
    Table(Group),
    TableType(Type),
    Choice(Group),
}

/// Format each different data type into a schema string.
impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Field::String(s) => write!(f, "\"{}\"", s),
            Field::Type(cddl_type) => write!(f, "{}", cddl_type),
            Field::Group(group) => write!(f, "{}", group),
            Field::Vector(group) => write!(f, "[* {} ]", format!("{}", group)),
            Field::Struct(group) => write!(f, "{{ {} }}", format!("{}", group)),
            Field::Table(group) => write!(f, "{{ + tstr => {{ {} }} }}", group),
            Field::TableType(value_type) => write!(f, "{{ + tstr => {} }}", value_type),
            Field::Choice(group) => write!(f, "&{}", group),
        }
    }
}

/// Struct for building and representing CDDL groups.
/// CDDL uses groups to define reuseable data structures
/// they can be merged into schema or used in Vectors, Tables and Structs
#[derive(Clone, Debug)]
pub struct Group {
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

#[derive(Clone, Debug)]
/// Schema struct for creating CDDL schemas, valdating MessageFields and creating messages
/// following the defined schema.
pub struct Schema {
    name: Option<String>,
    fields: Option<BTreeMap<String, Field>>,
    schema_hash: Option<Hash>,
    schema_string: Option<String>,
}

impl Schema {
    /// Create a new blank Schema
    pub fn new(name: String) -> Self {
        Self {
            name: Some(name),
            fields: Some(BTreeMap::new()),
            schema_hash: None,
            schema_string: None,
        }
    }

    /// Create a new Schema from a CDDL string
    pub fn new_from(schema_hash: &Hash, schema_str: &String) -> Result<Self, SchemaError> {
        let mut lexer = Lexer::new(schema_str);
        let parser = Parser::new(lexer.iter(), schema_str);
        let schema_string = match parser {
            Ok(mut parser) => match parser.parse_cddl() {
                Ok(cddl) => Ok(Some(cddl.to_string())),
                Err(err) => Err(SchemaError::ParsingError(err.to_string())),
            },
            Err(err) => Err(SchemaError::ParsingError(err.to_string())),
        }?;
        let schema_hash = match Hash::new(schema_hash.as_str()) {
            Ok(hash) => Ok(Some(hash)),
            Err(err_str) => Err(SchemaError::InvalidSchema(err_str.to_string())),
        }?;

        Ok(Self {
            name: None,
            fields: None,
            schema_hash,
            schema_string,
        })
    }

    /// Add a field definition to this schema
    pub fn add_message_field(&mut self, key: String, field_type: Type) -> Result<(), SchemaError> {
        // Match passed type and map it to our MessageFields type and CDDL types (do we still need the
        // MessageFields type key when we are using schemas?)
        let type_string = match field_type {
            Type::Tstr => "str",
            Type::Int => "int",
            Type::Float => "float",
            Type::Bool => "bool",
            Type::Relation => "relation",
        };

        // Create a message field group and add fields
        let mut message_fields = Group::new(key.to_owned());
        message_fields.add_field("type", Field::String(type_string.to_owned()));
        message_fields.add_field("value", Field::Type(field_type));

        // Format message fields group as a struct
        let message_fields = Field::Struct(message_fields);

        // Insert new message field into Schema fields.
        // If this Schema was created from a cddl string
        // `fields` will be None.
        match self.fields.clone() {
            Some(mut fields) => {
                fields.insert(key, message_fields);
                self.fields = Some(fields);
                Ok(())
            }
            None => Err(SchemaError::NoSchema),
        }
    }

    /// Create a new CREATE message validated against this schema
    pub fn create(
        &self,
        schema_hash: &str,
        key_values: Vec<(&str, &str)>,
    ) -> Result<Message, SchemaError> {
        let schema = match Hash::new(schema_hash) {
            Ok(hash) => Ok(hash),
            Err(err_str) => Err(SchemaError::InvalidSchema(err_str.to_string())),
        };

        let mut fields = MessageFields::new();

        for (key, value) in key_values {
            match fields.add(key, MessageValue::Text(value.into())) {
                Ok(_) => Ok(()),
                Err(err_str) => Err(SchemaError::InvalidSchema(err_str.to_string())),
            }?;
        }

        match self.validate_message(serde_cbor::to_vec(&fields.clone()).unwrap()) {
            Ok(_) => Ok(()),
            Err(err_str) => Err(SchemaError::ValidationError(err_str.to_string())),
        }?;

        match Message::new_create(schema.unwrap(), fields) {
            Ok(hash) => Ok(hash),
            Err(err_str) => Err(SchemaError::InvalidSchema(err_str.to_string())),
        }
    }

    /// Create a new UPDATE message validated against this schema
    pub fn update(
        &self,
        schema_hash: &str,
        id: &str,
        key_values: Vec<(&str, &str)>,
    ) -> Result<Message, SchemaError> {
        let schema = match Hash::new(schema_hash) {
            Ok(hash) => Ok(hash),
            Err(err_str) => Err(SchemaError::InvalidSchema(err_str.to_string())),
        };

        let mut fields = MessageFields::new();
        let id = Hash::new(id).unwrap();

        for (key, value) in key_values {
            match fields.add(key, MessageValue::Text(value.into())) {
                Ok(_) => Ok(()),
                Err(err_str) => Err(SchemaError::InvalidSchema(err_str.to_string())),
            }?;
        }

        match self.validate_message(serde_cbor::to_vec(&fields.clone()).unwrap()) {
            Ok(_) => Ok(()),
            Err(err_str) => Err(SchemaError::ValidationError(err_str.to_string())),
        }?;

        match Message::new_update(schema.unwrap(), id, fields) {
            Ok(hash) => Ok(hash),
            Err(err_str) => Err(SchemaError::InvalidSchema(err_str.to_string())),
        }
    }

    /// Validate a message against this user schema
    #[cfg(not(target_arch = "wasm32"))]
    pub fn validate_message(&self, bytes: Vec<u8>) -> Result<(), SchemaError> {
        match validate_cbor_from_slice(&format!("{}", self), &bytes) {
            Err(cbor::Error::Validation(err)) => {
                let err_str = err
                    .iter()
                    .map(|fe| format!("{}: \"{}\"", fe.cbor_location, fe.reason))
                    .collect::<Vec<String>>()
                    .join(", ");

                Err(SchemaError::InvalidSchema(err_str))
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
        match &self.fields {
            Some(map) => {
                let name = match self.name.as_ref() {
                    Some(name) => Ok(name),
                    // Need custom errors, but how to do that in Display?
                    None => Err(fmt::Error),
                }?;
                write!(f, "{} = {{ ", name)?;
                for (count, value) in map.iter().enumerate() {
                    if count != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", value.0, value.1)?;
                }
                write!(f, " }}\n")
            }
            None => {
                match &self.schema_string {
                    Some(s) => write!(f, "{}", s),
                    // Should this throw an error or just be an empty schema, like so?
                    None => write!(f, ""),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::hash::Hash;
    use crate::message::{MessageFields, MessageValue};
    use crate::schema::{Schema, Type, USER_SCHEMA, USER_SCHEMA_HASH};

    #[test]
    pub fn schema_builder() {
        // Instanciate new empty schema named "person"
        let mut person = Schema::new("person".to_owned());

        // Add two message fields to the schema
        person
            .add_message_field("name".to_owned(), Type::Tstr)
            .unwrap();
        person
            .add_message_field("age".to_owned(), Type::Int)
            .unwrap();

        // Create a new "person" message
        let mut me = MessageFields::new();
        me.add("name", MessageValue::Text("Sam".to_owned()))
            .unwrap();
        me.add("age", MessageValue::Integer(35)).unwrap();

        // Validate message fields against person schema
        let me_bytes = serde_cbor::to_vec(&me).unwrap();
        assert!(person.validate_message(me_bytes.clone()).is_ok());
    }

    #[test]
    pub fn schema_from_string() {
        // Create a new "person" message
        let mut me = MessageFields::new();
        me.add("name", MessageValue::Text("Sam".to_owned()))
            .unwrap();
        me.add("age", MessageValue::Integer(35)).unwrap();

        // Instanciate "person" schema from cddl string
        let cddl_str = "person = { (
            age: { type: \"int\", value: int }, 
            name: { type: \"str\", value: tstr } 
        ) }";

        let person_from_string =
            Schema::new_from(&Hash::new(USER_SCHEMA_HASH).unwrap(), &cddl_str.to_string()).unwrap();

        // Validate message fields against person schema
        let me_bytes = serde_cbor::to_vec(&me).unwrap();
        assert!(person_from_string.validate_message(me_bytes).is_ok());
    }

    #[test]
    pub fn validate_against_megaschema() {
        // Instanciate global user schema from mega schema string and it's hash
        let user_schema = Schema::new_from(
            &Hash::new(USER_SCHEMA_HASH).unwrap(),
            &USER_SCHEMA.to_string(),
        )
        .unwrap();

        let mut me = MessageFields::new();
        me.add("name", MessageValue::Text("Sam".to_owned()))
            .unwrap();
        me.add("age", MessageValue::Integer(35)).unwrap();

        let mut my_address = MessageFields::new();
        my_address
            .add("house-number", MessageValue::Integer(8))
            .unwrap();
        my_address
            .add("street", MessageValue::Text("Panda Lane".to_owned()))
            .unwrap();
        my_address
            .add("city", MessageValue::Text("Bamboo Town".to_owned()))
            .unwrap();

        // Validate message fields against user schema
        let me_bytes = serde_cbor::to_vec(&me).unwrap();
        let my_address_bytes = serde_cbor::to_vec(&my_address).unwrap();

        assert!(user_schema.validate_message(me_bytes).is_ok());
        assert!(user_schema.validate_message(my_address_bytes).is_ok());

        // Messages not matching one of the user schema should fail
        let mut naughty_panda = MessageFields::new();
        naughty_panda
            .add("name", MessageValue::Text("Naughty Panda".to_owned()))
            .unwrap();
        naughty_panda
            .add("colour", MessageValue::Text("pink & orange".to_owned()))
            .unwrap();

        let naughty_panda_bytes = serde_cbor::to_vec(&naughty_panda).unwrap();
        assert!(user_schema.validate_message(naughty_panda_bytes).is_err());
    }
}
