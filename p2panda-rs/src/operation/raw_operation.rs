// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::btree_map::Iter;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::document::{DocumentId, DocumentViewId};
use crate::operation::{
    AsOperation, Operation, OperationAction, OperationFields, OperationValue, OperationVersion,
    RawOperationError,
};
use crate::schema::{FieldName, SchemaId};
use crate::Validate;

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct RawFields(BTreeMap<FieldName, RawValue>);

impl RawFields {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns true when no field is given.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of fields.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, name: &str) -> Option<&RawValue> {
        self.0.get(name)
    }

    pub fn insert(&mut self, name: &str, value: RawValue) -> Result<(), RawOperationError> {
        if self.0.contains_key(name) {
            // @TODO
            panic!("Duplicate")
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    pub fn iter(&self) -> Iter<FieldName, RawValue> {
        self.0.iter()
    }
}

impl From<&OperationFields> for RawFields {
    fn from(fields: &OperationFields) -> Self {
        let mut raw = RawFields::new();

        for (name, value) in fields.iter() {
            let raw_value = match value {
                OperationValue::Boolean(bool) => RawValue::Boolean(*bool),
                OperationValue::Integer(int) => RawValue::Integer(*int),
                OperationValue::Float(float) => RawValue::Float(*float),
                OperationValue::Text(str) => RawValue::Text(str.to_owned()),
                OperationValue::Relation(relation) => {
                    RawValue::Relation(relation.document_id().to_owned())
                }
                OperationValue::RelationList(list) => RawValue::RelationList(list.sorted()),
                OperationValue::PinnedRelation(relation) => {
                    RawValue::PinnedRelation(relation.view_id().to_owned())
                }
                OperationValue::PinnedRelationList(list) => {
                    RawValue::PinnedRelationList(list.sorted())
                }
            };

            raw.insert(&name, raw_value);
        }

        raw
    }
}

impl Validate for RawFields {
    type Error = RawOperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // @TODO
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum RawValue {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    Text(String),
    Relation(DocumentId),
    PinnedRelation(DocumentViewId),
    RelationList(Vec<DocumentId>),
    PinnedRelationList(Vec<DocumentViewId>),
}

impl RawValue {
    pub fn field_type(&self) -> &str {
        match self {
            RawValue::Boolean(_) => "bool",
            RawValue::Integer(_) => "int",
            RawValue::Float(_) => "float",
            RawValue::Text(_) => "str",
            RawValue::Relation(_) => "relation",
            RawValue::RelationList(_) => "relation_list",
            RawValue::PinnedRelation(_) => "pinned_relation",
            RawValue::PinnedRelationList(_) => "pinned_relation_list",
        }
    }
}

impl Validate for RawValue {
    type Error = RawOperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // @TODO
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct RawOperation(
    OperationVersion,
    OperationAction,
    #[serde(skip_serializing_if = "Option::is_none")] Option<DocumentViewId>,
    SchemaId,
    #[serde(skip_serializing_if = "Option::is_none")] Option<RawFields>,
);

impl RawOperation {
    pub fn version(&self) -> OperationVersion {
        self.0
    }

    pub fn action(&self) -> OperationAction {
        self.1
    }

    pub fn previous_operations(&self) -> Option<&DocumentViewId> {
        self.2.as_ref()
    }

    pub fn schema_id(&self) -> &SchemaId {
        &self.3
    }

    pub fn fields(&self) -> Option<&RawFields> {
        self.4.as_ref()
    }
}

impl From<&Operation> for RawOperation {
    fn from(operation: &Operation) -> Self {
        RawOperation(
            operation.version(),
            operation.action(),
            operation.previous_operations(),
            operation.schema(),
            operation.fields().as_ref().map(|fields| fields.into()),
        )
    }
}

impl Validate for RawOperation {
    type Error = RawOperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // @TODO
        Ok(())
    }
}
