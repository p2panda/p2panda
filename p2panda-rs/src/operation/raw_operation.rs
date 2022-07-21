// SPDX-License-Identifier: AGPL-3.0-or-later

use std::slice::Iter;

use serde::{Deserialize, Serialize};

use crate::document::{DocumentId, DocumentViewId};
use crate::operation::{
    AsOperation, Operation, OperationAction, OperationError, OperationFields, OperationValue,
    OperationVersion,
};
use crate::schema::{FieldName, SchemaId};
use crate::Validate;

pub type RawField = (FieldName, RawValue);

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct RawFields(Vec<RawField>);

impl RawFields {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn find(&self, name: &str) -> Option<&RawField> {
        self.0.iter().find(|field| field.0 == name)
    }

    pub fn insert(&mut self, name: &str, value: &RawValue) {
        self.0.push((name.to_owned(), value.clone()))
    }

    pub fn iter(&self) -> Iter<RawField> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl From<&OperationFields> for RawFields {
    fn from(fields: &OperationFields) -> Self {
        let mut raw = RawFields::new();

        for (name, value) in fields.iter() {
            let raw_value = match *value {
                OperationValue::Boolean(bool) => RawValue::Boolean(bool),
                OperationValue::Integer(int) => RawValue::Integer(int),
                OperationValue::Float(float) => RawValue::Float(float),
                OperationValue::Text(str) => RawValue::Text(str),
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

            raw.insert(name, &raw_value);
        }

        raw
    }
}

impl Validate for RawFields {
    type Error = OperationError;

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

impl Validate for RawValue {
    type Error = OperationError;

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
    pub fn version(&self) -> &OperationVersion {
        &self.0
    }

    pub fn action(&self) -> &OperationAction {
        &self.1
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
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // @TODO
        Ok(())
    }
}
