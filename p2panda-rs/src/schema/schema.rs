// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::document::DocumentViewId;
use crate::schema::system::{SchemaFieldView, SchemaView};
use crate::schema::SchemaError;

/// The key of a schema field
type FieldKey = String;

/// A struct representing a materialised schema.
///
/// It is constructed from a `SchemaView` and all related `SchemaFieldView`s.
#[derive(Debug, PartialEq)]
pub struct Schema {
    id: DocumentViewId,
    name: String,
    description: String,
    fields: BTreeMap<FieldKey, SchemaFieldView>,
}

impl Schema {
    /// Instantiate a new `Schema` from a `SchemaView` and it's `SchemaFieldView`s
    pub fn new(schema: SchemaView, fields: Vec<SchemaFieldView>) -> Result<Schema, SchemaError> {
        // Validate that the passed `SchemaFields` are the correct ones for this `Schema`.
        for schema_field in schema.fields().iter() {
            match fields
                .iter()
                .find(|schema_field_view| schema_field_view.id() == &schema_field)
            {
                Some(_) => Ok(()),
                None => Err(SchemaError::InvalidFields),
            }?;
        }

        // And that no extra fields were passed
        if fields.iter().len() > schema.fields().iter().len() {
            return Err(SchemaError::InvalidFields);
        }

        // Construct a key-value map of fields
        let mut fields_map = BTreeMap::new();
        for field in fields {
            fields_map.insert(field.name().to_string(), field);
        }

        Ok(Schema {
            id: schema.view_id().to_owned(),
            name: schema.name().to_owned(),
            description: schema.description().to_owned(),
            fields: fields_map,
        })
    }
}
