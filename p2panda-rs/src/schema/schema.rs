use std::collections::BTreeMap;
use std::fmt;

use cddl::lexer::Lexer;
use cddl::parser::Parser;
#[cfg(not(target_arch = "wasm32"))]
use cddl::validate_cbor_from_slice;
#[cfg(not(target_arch = "wasm32"))]
use cddl::validator::cbor;

use super::error::SchemaError;

/// CDDL types
#[derive(Clone, Debug)]
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

/// Field types
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

/// Struct for building and representing CDDL groups
// CDDL uses groups to define reuseable data structures
// they can be merged into schema or used in Vectors, Tables and Structs
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

pub fn create_message_field(key: &str, field_type: Type) -> Field {
    // Match passed type and map it to our MessageFields type and CDDL types (do we still need the
    // MessageFields type key when we are using schemas?)
    let type_string = match field_type {
        Type::Tstr => "str",
        Type::Int => "int",
        Type::Float => "float",
        Type::Bool => "bool",
        Type::Relation => "relation",
    };
    // Create an array of message_fields
    let mut message_fields = Group::new(key.to_owned());
    message_fields.add_field("type", Field::String(type_string.to_owned()));
    message_fields.add_field("value", Field::Type(field_type));
    Field::Struct(message_fields)
}

#[derive(Debug)]
/// UserSchema struct for creating CDDL schemas and valdating MessageFields
pub struct UserSchema {
    name: Option<String>,
    fields: Option<BTreeMap<String, Field>>,
    schema_string: Option<String>,
}

impl UserSchema {
    /// Create a new blank UserSchema
    pub fn new(name: String) -> Self {
        Self {
            name: Some(name),
            fields: Some(BTreeMap::new()),
            schema_string: None,
        }
    }

    /// Create a new UserSchema from a CDDL string
    // Instanciate a new UserSchema instance from a CDDL string.
    pub fn new_from_string(schema: &String) -> Result<Self, SchemaError> {
        let mut lexer = Lexer::new(schema);
        let parser = Parser::new(lexer.iter(), schema);
        let cddl_string = match parser {
            Ok(mut parser) => match parser.parse_cddl() {
                Ok(cddl) => Ok(cddl.to_string()),
                Err(err) => Err(SchemaError::ParsingError(err.to_string())),
            },
            Err(err) => Err(SchemaError::ParsingError(err.to_string())),
        };
        Ok(Self {
            name: None,
            fields: None,
            schema_string: Some(cddl_string.unwrap()),
        })
    }

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

        // Insert new message field into UserSchema fields.
        // If this UserSchema was created from a cddl string
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

impl fmt::Display for UserSchema {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.fields {
            Some(map) => {
                // Naughty unwrap here needs to go!
                write!(f, "{} = {{ ", self.name.as_ref().unwrap())?;
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
    use crate::message::{MessageFields, MessageValue};

    use super::{Type, UserSchema};

    #[test]
    pub fn validate_message() {
        
        // Instanciate new empty schema named "person"
        let mut person = UserSchema::new("person".to_owned());

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

        // Instanciate "person" schema from cddl string 
        let cddl_str = "person = { 
            age: { ( type: \"int\", value: int ) }, 
            name: { ( type: \"str\", value: tstr ) } 
        }";
        
        let person_from_string =
            UserSchema::new_from_string(&cddl_str.to_string()).unwrap();

        // Both schemas should match
        assert_eq!(format!("{}", person_from_string), format!("{}", person));
        assert!(person_from_string.validate_message(me_bytes).is_ok());
    }
}
