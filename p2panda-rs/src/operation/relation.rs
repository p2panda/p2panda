// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::document::DocumentId;
use crate::hash::Hash;
use crate::operation::OperationError;
use crate::Validate;

/// Field type representing references to other documents.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Relation(DocumentId);

impl Relation {
    /// Returns a new relation field type.
    pub fn new(document: DocumentId) -> Self {
        Self(document)
    }

    /// Returns the relations document id.
    pub fn document_id(&self) -> &DocumentId {
        &self.0
    }
}

impl Validate for Relation {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()?;
        Ok(())
    }
}

/// Reference to the exact version of the document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct PinnedRelation(Vec<Hash>);

impl PinnedRelation {
    /// Returns a new relation field type.
    pub fn new(document_view: Vec<Hash>) -> Self {
        Self(document_view)
    }
}

impl Validate for PinnedRelation {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        for operation_id in &self.0 {
            operation_id.validate()?;
        }

        Ok(())
    }
}

/// A `RelationList` can be used to reference multiple foreign documents from a document field.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelationList(Vec<Relation>);

impl RelationList {
    pub fn new(relations: Vec<Relation>) -> Self {
        Self(relations)
    }
}

impl Validate for RelationList {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        for operation_id in &self.0 {
            operation_id.validate()?;
        }

        Ok(())
    }
}
