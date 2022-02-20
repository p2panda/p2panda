// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::document::DocumentViewId;

use super::system_schema::{SchemaFieldView, SchemaView};

/// Construct a `Schema` from a schema view and it's associated schema fields.
pub fn build_schema(schema: SchemaView, fields: Vec<SchemaFieldView>) -> Schema {
    let mut fields_map = BTreeMap::new();

    for field in fields {
        fields_map.insert(field.name().to_string(), field);
    }
    Schema {
        id: schema.id().to_owned(),
        name: schema.name().to_owned(),
        description: schema.description().to_owned(),
        fields: fields_map,
    }
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
