use std::collections::BTreeMap;
use std::fmt;

use cddl::validate_cbor_from_slice;
use thiserror::Error;

/// Our very descriptive error
#[derive(Error, Debug)]
pub enum SchemaError {
    #[error("Some error happened")]
    Error,
}

/// CDDL types
#[derive(Clone, Debug)]
pub enum CDDLType {
    Bool,
    Uint,
    Nint,
    Int,
    Float16,
    Float32,
    Float64,
    Float,
    Bstr,
    Tstr,
    Const(String),
}

/// CDDL schema type string formats
impl fmt::Display for CDDLType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let cddl_type = match self {
            CDDLType::Bool => "bool",
            CDDLType::Uint => "uint",
            CDDLType::Nint => "nint",
            CDDLType::Int => "int",
            CDDLType::Float16 => "float16",
            CDDLType::Float32 => "float32",
            CDDLType::Float64 => "float64",
            CDDLType::Float => "float",
            CDDLType::Bstr => "bstr",
            CDDLType::Tstr => "tstr",
            // This isn't exactly a CDDL type,
            // it's for string constants.
            CDDLType::Const(str) => str,
        };

        write!(f, "{}", cddl_type)
    }
}

/// Struct for building and representing CDDL groups
// CDDL uses groups to define reuseable data structures
// they can be merged into schema or used in Arrays, Tables and Structs
#[derive(Clone, Debug)]
pub struct CDDLGroup {
    name: String,
    fields: BTreeMap<String, CDDLEntry>,
}

impl CDDLGroup {
    /// Create a new CDDL group
    pub fn new(name: String) -> Self {
        Self {
            name,
            fields: BTreeMap::new(),
        }
    }

    /// Add an CDDLEntry to the group.
    pub fn add_entry(&mut self, key: &str, value_type: CDDLEntry) -> Result<(), SchemaError> {
        self.fields.insert(key.to_owned(), value_type);
        Ok(())
    }
}

impl fmt::Display for CDDLGroup {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let map = &self.fields;
        write!(f, "( ")?;
        for (count, value) in map.iter().enumerate() {
            // For every element except the first, add a comma.
            if count != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{} : {}", value.0, value.1)?;
        }
        write!(f, " )")
    }
}

/// A CDDL key value pair, I think they call this an Entry, where the value can be a Type, Struct, Table, Array or Group.
#[derive(Clone, Debug)]
pub enum CDDLEntry {
    Group(CDDLGroup),
    Array(CDDLGroup),
    Struct(CDDLGroup),
    Table(CDDLGroup),
    Type(CDDLType),
}

/// Format each different data type into a schema string.
impl fmt::Display for CDDLEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CDDLEntry::Group(group) => {
                write!(f, "{:?}", group)
            }
            CDDLEntry::Array(group) => {
                write!(f, "[* {} ]", format!("{}", group))
            }
            CDDLEntry::Struct(group) => {
                write!(f, "{{ {} }}", format!("{}", group))
            }
            CDDLEntry::Table(_) => write!(f, "{}", "table"),
            CDDLEntry::Type(value_type) => {
                // Hack to catch "tstr" types and format to "str"
                write!(f, "{}", format!("{}", value_type))
            }
        }
    }
}

#[derive(Debug)]
/// UserSchema struct for creating CDDL schemas and valdating MessageFields
pub struct UserSchema {
    name: String,
    schema: BTreeMap<String, CDDLEntry>,
}

impl UserSchema {
    /// Create a new blank UserSchema
    pub fn new(name: String) -> Self {
        Self {
            name,
            schema: BTreeMap::new(),
        }
    }

    /// Create a new UserSchema from a CDDL string
    pub fn new_from_string(_cddl_schema: String) -> Result<(), SchemaError> {
        // TBC: Do the stuff here
        Ok(())
    }

    /// Add an entry to this schema
    pub fn add_entry(&mut self, key: &str, value: CDDLEntry) -> Result<(), SchemaError> {
        match value {
            CDDLEntry::Array(_)
            | CDDLEntry::Struct(_)
            | CDDLEntry::Table(_)
            | CDDLEntry::Type(_) => {
                // Insert Array entry
                self.schema.insert(key.to_owned(), value);
                Ok(())
            }
            CDDLEntry::Group(_) => {
                // Groups should be merged via the special method
                Ok(())
            }
        }
    }

    /// Validate a message against this user schema
    pub fn validate_message(&self, bytes: Vec<u8>) -> Result<(), cddl::validator::cbor::Error> {
        validate_cbor_from_slice(&format!("{}", self), &bytes)
    }
}

impl fmt::Display for UserSchema {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let map = &self.schema;
        write!(f, "{} = {{ ", self.name)?;
        for (count, value) in map.iter().enumerate() {
            if count != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{} : {}", value.0, value.1)?;
        }
        write!(f, " }}")
    }
}

#[cfg(test)]
mod tests {
    use crate::message::{MessageFields, MessageValue};

    use super::{CDDLEntry, CDDLGroup, CDDLType, UserSchema};

    #[test]
    pub fn validate_cbor() {
        // Construct an "age" message field
        let mut message_field_age = CDDLGroup::new("age".to_owned());
        message_field_age
            .add_entry(
                "type",
                CDDLEntry::Type(CDDLType::Const(r#""int""#.to_owned())),
            )
            .unwrap();
        message_field_age
            .add_entry("value", CDDLEntry::Type(CDDLType::Int))
            .unwrap();

        // Construct a "name" message field
        let mut message_field_name = CDDLGroup::new("name".to_owned());
        message_field_name
            .add_entry(
                "type",
                CDDLEntry::Type(CDDLType::Const(r#""str""#.to_owned())),
            )
            .unwrap();
        message_field_name
            .add_entry("value", CDDLEntry::Type(CDDLType::Tstr))
            .unwrap();

        // Construct a "person" user schema using the above groups
        let mut person_schema = UserSchema::new("person_schema".to_owned());

        person_schema
            .add_entry("age", CDDLEntry::Struct(message_field_age))
            .unwrap();
        person_schema
            .add_entry("name", CDDLEntry::Struct(message_field_name))
            .unwrap();

        print!("{}", person_schema);
        // => person = { age : { type: "int", value: int }, name : { type: "str", value: tstr } }

        // Build "person" message fields
        let mut me = MessageFields::new();
        me.add("name", MessageValue::Text("Sam".to_owned()))
            .unwrap();
        me.add("age", MessageValue::Integer(35)).unwrap();

        // Encode message fields
        let me_encoded = serde_cbor::to_vec(&me).unwrap();

        // Validate message fields against person schema
        assert!(person_schema.validate_message(me_encoded).is_ok());
    }
}
