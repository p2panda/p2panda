use std::collections::BTreeMap;

use crate::hash::Hash;

use thiserror::Error;

/// Custom error types for schema validation.
#[derive(Error, Debug)]
pub enum SchemaError {
    /// Our very descriptive error
    #[error("Some error happened")]
    Error,
}

#[derive(Debug)]
pub enum CDDLType {
    // CDDL types and structs
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
}

#[derive(Debug)]
pub struct CDDLGroup {
    // CDDL uses groups to define reuseable data structures
    // they are also used to express Arrays, Tables and Structs
    entries: BTreeMap<String, CDDLValue>,
}

impl CDDLGroup {
    // Create a new CDDL group
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
    // Add an `entry` (a term used in CDDL lingo for key/value_type paie) to the group.
    pub fn add_entry(&mut self, key: &str, value_type: CDDLValue) -> Result<(), SchemaError> {
        self.entries.insert(key.to_owned(), value_type);
        Ok(())
    }
}

#[derive(Debug)]
pub enum CDDLValue {
    // Possible CDDL values
    Group(CDDLGroup),
    Array(CDDLGroup),
    Struct(CDDLGroup),
    Table(CDDLGroup),
    Type(CDDLType),
}

#[derive(Debug)]
pub struct CDDLSchema {
    // A CDDL schema can include key/value_type entries or groups
    entries: BTreeMap<String, CDDLValue>,
}

impl CDDLSchema {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn add_entry(&mut self, key: &str, value: CDDLValue) -> Result<(), SchemaError> {
        // Add an entry to a CDDL schema
        // This can be a simple key/pair or CDDL struct represented by a group

        match value {
            CDDLValue::Array(_) => {
                // Insert Array entry
                Ok(())},
            CDDLValue::Struct(_) => {
                // Insert Struct entry
                Ok(())},
            CDDLValue::Table(_) => {
                // Insert Table entry
                Ok(())},
            CDDLValue::Type(_) => {
                // Insert key value entry
                self.entries.insert(key.to_owned(), value);
                Ok(())
            }
            CDDLValue::Group(_) => {
                // Groups should be merged via the special method
                Err(SchemaError::Error)},
        }
    }

    pub fn add_group(&mut self, group: CDDLGroup) -> Result<(), SchemaError> {
        // Add a group to a CDDL schema, this merges all group entries into the schema
        for (key, value) in group.entries {
            self.entries.insert(key.to_owned(), value);
        }
        Ok(())
    }

    pub fn as_string(&self) -> String {
        // Awesome logic for generating string from our CDDLSchema struct goes here
        String::from("THIS_IS_A_CDDL_SCHEMA_STRING")
    }
}

#[derive(Debug)]
pub struct UserSchema {
    /// Describes if this message creates, updates or deletes data.
    schema: String,
}

impl UserSchema {
    /// Creates a new UserSchema instance from a schema hash
    pub fn new(schema: Hash) -> Result<Self, SchemaError> {
        // Decode and validate schema hash
        let schema_str = schema.as_str();
        let user_schema = Self {
            schema: schema_str.to_owned(),
        };
        user_schema.validate()?;

        Ok(user_schema)
    }

    /// Creates a new UserSchema instance from a CDDL schema string
    pub fn new_from_string(schema_str: String) -> Result<Self, SchemaError> {
        // validate schema str here
        let user_schema = Self { schema: schema_str };

        user_schema.validate()?;

        Ok(user_schema)
    }

    /// Validate user schema
    pub fn validate(&self) -> Result<(), SchemaError> {
        // validation schema against meta schema....
        Ok(())
    }

    /// Checks CBOR bytes against CDDL schemas.
    pub fn validate_message(&self, bytes: Vec<u8>) -> Result<(), SchemaError> {
        // validation bytes against instance schema....
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::hash::Hash;
    use crate::message::{MessageFields, MessageValue};

    use super::{CDDLGroup, CDDLSchema, CDDLType, CDDLValue, UserSchema};

    #[test]
    fn create_schema() {
        let mut house = CDDLSchema::new();
        house
            .add_entry("number", CDDLValue::Type(CDDLType::Int))
            .unwrap();

        let mut person = CDDLGroup::new();
        person
            .add_entry("name", CDDLValue::Type(CDDLType::Tstr))
            .unwrap();
        person
            .add_entry("age", CDDLValue::Type(CDDLType::Int))
            .unwrap();

        house.add_entry("owner", CDDLValue::Group(person)).unwrap();

        println!("{:?}", house);

        // => CDDLSchema { entries: {"number": Type(Int), "owner": Group(CDDLGroup { entries: {"age": Type(Int), "name": Type(Tstr)} })} }
        // We would have a nice way to convert this to a CDDL string
        // CDDL implements many schema definition features which we'd need to support
        // There must be a library which does this for us... but it's fun building it for now

        let house_schema = UserSchema::new_from_string(house.as_string()).unwrap();

        // SCHEMA PUBLISHED TO NETWORK //
        // here we publish the house schema to an aquadoggo node.
        // Later, when we want to create a house
        // we retrieve the schema hash (somehow?) and validate against it

        let house_schema_hash = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";

        let retrieved_house_schema =
            UserSchema::new(Hash::new(house_schema_hash).unwrap()).unwrap();

        let mut my_house = MessageFields::new();
        my_house.add("number", MessageValue::Integer(12)).unwrap();

        let mut me = MessageFields::new();
        me.add("name", MessageValue::Text("Sam".to_owned()))
            .unwrap();
        me.add("age", MessageValue::Integer(35)).unwrap();

        // my_house.add("owner", me).unwrap();
        // arrays and maps in MessageFields aren't implemented yet

        // Validate massage fields against retrieved_house_schema

        // retrieved_house_schema
        //     .validate_message(my_house.to_cbor())
        //     .unwrap();
    }
}
