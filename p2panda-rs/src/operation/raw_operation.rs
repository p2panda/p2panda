// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::document::DocumentViewId;
use crate::operation::{
    AsOperation, Operation, OperationAction, OperationFields, OperationValue, OperationVersion,
};
use crate::schema::SchemaId;

#[derive(Deserialize, Serialize, Debug, PartialEq)]
struct RawFields(Vec<(String, RawValue)>);

impl RawFields {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn insert(&mut self, name: &str, value: &RawValue) {
        self.0.push((name.to_owned(), value.clone()))
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
enum RawValue {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    Text(String),
    RelationList(Vec<String>),
    PinnedRelationList(Vec<Vec<String>>),
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct RawOperation(
    OperationVersion,
    OperationAction,
    #[serde(skip_serializing_if = "Option::is_none")] Option<DocumentViewId>,
    SchemaId,
    #[serde(skip_serializing_if = "Option::is_none")] Option<RawFields>,
);

impl From<&Operation> for RawOperation {
    fn from(operation: &Operation) -> Self {
        RawOperation(
            operation.version(),
            operation.action(),
            operation.previous_operations(),
            operation.schema(),
            operation.fields().map(|fields| fields_to_raw(&fields)),
        )
    }
}

fn fields_to_raw(fields: &OperationFields) -> RawFields {
    let mut raw = RawFields::new();

    for (name, value) in fields.iter() {
        let raw_value = match *value {
            OperationValue::Boolean(bool) => RawValue::Boolean(bool),
            OperationValue::Integer(int) => RawValue::Integer(int),
            OperationValue::Float(float) => RawValue::Float(float),
            OperationValue::Text(str) => RawValue::Text(str),
            OperationValue::Relation(id) => RawValue::Text(id.as_str().to_owned()),
            OperationValue::RelationList(list) => RawValue::RelationList(
                list.sorted()
                    .iter()
                    .map(|id| id.as_str().to_owned())
                    .collect(),
            ),
            OperationValue::PinnedRelation(view_id) => RawValue::Text(view_id.as_str().to_owned()),
            OperationValue::PinnedRelationList(list) => RawValue::PinnedRelationList(
                list.iter()
                    .map(|relation| {
                        relation
                            .sorted()
                            .iter()
                            .map(|view_id| view_id.as_str().to_owned())
                            .collect()
                    })
                    .collect(),
            ),
        };

        raw.insert(name, &raw_value);
    }

    raw
}
