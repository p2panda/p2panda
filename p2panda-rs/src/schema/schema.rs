// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::document::DocumentViewId;
use crate::hash::Hash;

use super::{
    error::SchemaError,
    system_schema::{SchemaFieldView, SchemaView},
};

/// Construct a `Schema` from a schema view and it's associated schema fields.
#[allow(dead_code)] // These methods aren't used yet...
pub fn build_schema(
    schema: SchemaView,
    fields: Vec<SchemaFieldView>,
) -> Result<Schema, SchemaError> {
    // Collection the ids of the passed fields
    let field_ids: Vec<Hash> = fields
        .iter()
        .map(|field| field.id().document_id().to_owned())
        .collect();

    // Compare with the expected field ids
    // (eventually we will compare DocumentViewIds when we have the relation-list type)
    if field_ids.as_slice() != schema.fields() {
        return Err(SchemaError::InvalidFields);
    }

    // Construct the fields map
    let mut fields_map = BTreeMap::new();
    for field in fields {
        fields_map.insert(field.name().to_string(), field);
    }

    Ok(Schema {
        id: schema.id().to_owned(),
        name: schema.name().to_owned(),
        description: schema.description().to_owned(),
        fields: fields_map,
    })
}

/// A struct representing a materialised schema. It is constructed from a
/// SchemaView and all related SchemaFieldViews.
#[derive(Debug, PartialEq)]
pub struct Schema {
    id: DocumentViewId,
    name: String,
    description: String,
    fields: BTreeMap<String, SchemaFieldView>,
}
