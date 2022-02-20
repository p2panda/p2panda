// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::fmt;

use crate::schema::SchemaError;

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

impl ToString for SchemaBuilder {
    fn to_string(&self) -> String {
        let mut cddl_str = "".to_string();
        cddl_str += &format!("{} = {{ ", self.name);
        for (count, value) in self.fields.iter().enumerate() {
            if count != 0 {
                cddl_str += ", ";
            }
            cddl_str += &format!("{}: {}", value.0, value.1);
        }
        cddl_str += " }";
        cddl_str
    }
}

#[cfg(test)]
mod tests {
    use crate::operation::{OperationFields, OperationValue};

    use super::{SchemaBuilder, Type};

    // Only `person` schema.
    pub const PERSON_SCHEMA: &str = r#"person = { age: { ( type: "int", value: int ) }, name: { ( type: "str", value: tstr ) } }"#;
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
        assert_eq!(person.to_string(), PERSON_SCHEMA);
    }
}
