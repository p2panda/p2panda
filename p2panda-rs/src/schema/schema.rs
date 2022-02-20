// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::document::DocumentViewId;

use super::system_schema::SchemaFieldView;

/// A struct representing a schema with it's associated schema fields. It is
/// constructed from a SchemaView and all related SchemaFieldViews.
#[derive(Debug, PartialEq)]
pub struct Schema {
    id: DocumentViewId,
    name: String,
    description: String,
    fields: BTreeMap<String, SchemaFieldView>,
}
