use std::collections::BTreeMap;
use std::fmt;

#[cfg(not(target_arch = "wasm32"))]
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
pub enum Type {
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
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let cddl_type = match self {
            Type::Bool => "bool",
            Type::Uint => "uint",
            Type::Nint => "nint",
            Type::Int => "int",
            Type::Float16 => "float16",
            Type::Float32 => "float32",
            Type::Float64 => "float64",
            Type::Float => "float",
            Type::Bstr => "bstr",
            Type::Tstr => "tstr",
            // This isn't exactly a CDDL type,
            // it's for string constants.
            Type::Const(str) => str,
        };

        write!(f, "{}", cddl_type)
    }
}

#[derive(Clone, Debug)]
pub enum CDDLEntry {
    Type(Type),
    Group(Group),
    Vector(Group),
    Struct(Group),
    Table(Group),
    TableType(Type),
    Choice(Group)
}

/// Format each different data type into a schema string.
impl fmt::Display for CDDLEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CDDLEntry::Type(cddl_type) => write!(f, "{}", cddl_type),
            CDDLEntry::Group(group) => write!(f, "{}", group),
            CDDLEntry::Vector(group) => write!(f, "[* {} ]", format!("{}", group)),
            CDDLEntry::Struct(group) => write!(f, "{{ {} }}", format!("{}", group)),
            CDDLEntry::Table(group) => write!(f, "{{ + tstr => {{ {} }} }}", group),
            CDDLEntry::TableType(value_type) => write!(f, "{{ + tstr => {} }}", value_type),
            CDDLEntry::Choice(group) => write!(f, "&{}", group),
        }
    }
}

/// Struct for building and representing CDDL groups
// CDDL uses groups to define reuseable data structures
// they can be merged into schema or used in Vectors, Tables and Structs
#[derive(Clone, Debug)]
pub struct Group {
    name: String,
    fields: BTreeMap<String, CDDLEntry>,
}

impl Group {
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

impl fmt::Display for Group {
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
            CDDLEntry::Type(_) => {
                self.schema.insert(key.to_owned(), value);
                Ok(())
            }
            CDDLEntry::Group(_) => {
                //Groups should be merged not inserted
                Ok(())
            }
            _ => {
                self.schema.insert(key.to_owned(), value);
                Ok(())
            }
        }
    }

    /// Validate a message against this user schema
    #[cfg(not(target_arch = "wasm32"))]
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

    use super::{CDDLEntry, Group, Type, UserSchema};

    #[test]
    pub fn validate_cbor() {
        // Construct an "age" message field
        let mut message_field_age = Group::new("age".to_owned());
        message_field_age
            .add_entry("type", CDDLEntry::Type(Type::Const(r#""int""#.to_owned())))
            .unwrap();
        message_field_age
            .add_entry("value", CDDLEntry::Type(Type::Int))
            .unwrap();

        // Construct a "name" message field
        let mut message_field_name = Group::new("name".to_owned());
        message_field_name
            .add_entry("type", CDDLEntry::Type(Type::Const(r#""str""#.to_owned())))
            .unwrap();
        message_field_name
            .add_entry("value", CDDLEntry::Type(Type::Tstr))
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

    #[test]
    pub fn schema_with_table() {
        // Construct a "name" message field
        let mut message_field_prices = Group::new("prices".to_owned());
        message_field_prices
            .add_entry(
                "type",
                CDDLEntry::Type(Type::Const(r#""table""#.to_owned())),
            )
            .unwrap();
        message_field_prices
            .add_entry("value", CDDLEntry::TableType(Type::Uint))
            .unwrap();

        // Construct a "person" user schema using the above groups
        let mut items_schema = UserSchema::new("items_schema".to_owned());

        items_schema
            .add_entry("prices", CDDLEntry::Struct(message_field_prices))
            .unwrap();

        let json_string = r#"{"prices": {"type": "table", "value": {"reinforcement": 958, "hunter": 4034, "Liz": 2020, "manicurists": 857}}}"#;

        assert!(cddl::validate_json_from_str(&format!("{}", items_schema), json_string).is_ok());

        let longer_json_string = r#"{"prices": {"type": "table", "value": {"reinforcement": 958, "hunter": 4034, "Liz": 2020, "manicurists": 857, "blablabla": 123, "yabawaba": 254}}}"#;

        assert!(
            cddl::validate_json_from_str(&format!("{}", items_schema), longer_json_string).is_ok()
        );
    }
    
    #[test]
    pub fn group_as_choice(){
        let mut colours = Group::new("colours".to_owned());
        colours.add_entry("black", CDDLEntry::Type(Type::Const(r#"1"#.to_owned()))).unwrap();
        colours.add_entry("red", CDDLEntry::Type(Type::Const(r#"2"#.to_owned()))).unwrap();
        colours.add_entry("green", CDDLEntry::Type(Type::Const(r#"3"#.to_owned()))).unwrap();
        
        let mut colour_schema = UserSchema::new("colour_schema".to_owned());

        colour_schema
            .add_entry("colour", CDDLEntry::Choice(colours))
            .unwrap();

        let json_string = r#"{"colour": 1}"#;
        
        // The CDDL here is valid, but actually the JSON data doesn't look how I'd imagine
        // I think I'm just not understanding how _choice_ works properly.
        assert!(cddl::validate_json_from_str(&format!("{}", colour_schema), json_string).is_ok());
    }
}
