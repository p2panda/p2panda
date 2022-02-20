use std::collections::BTreeMap;

use crate::hash::Hash;

use super::system_schema::{SchemaFieldView, SchemaView};

pub struct DocumentViewId {
    document_id: Hash,
    view_id: Vec<Hash>,
}
pub struct Schema {
    id: DocumentViewId,
    name: String,
    description: String,
    fields: BTreeMap<String, SchemaFieldView>,
}
