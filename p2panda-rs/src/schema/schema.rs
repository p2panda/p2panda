use std::collections::BTreeMap;

use crate::document::DocumentViewId;

use super::system_schema::SchemaFieldView;

pub struct Schema {
    id: DocumentViewId,
    name: String,
    description: String,
    fields: BTreeMap<String, SchemaFieldView>,
}
