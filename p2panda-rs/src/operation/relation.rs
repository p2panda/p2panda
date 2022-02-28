// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::operation::OperationError;
use crate::Validate;

// @TODO: Replace this with DocumentViewId
type GraphTips = Vec<Hash>;

/// Field type representing references to other documents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Relation(Hash);

impl Relation {
    pub fn new(hash: Hash) -> Self {
        Self(hash)
    }
}

impl Validate for Relation {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()?;
        Ok(())
    }
}

/// Field type representing references to other documents at a certain point in history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PinnedRelation(GraphTips);

impl PinnedRelation {
    pub fn new(graph_tips: GraphTips) -> Self {
        Self(graph_tips)
    }
}

impl Validate for PinnedRelation {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        for hash in &self.0 {
            hash.validate()?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelationList(Vec<Hash>);

impl RelationList {
    pub fn new(hashes: Vec<Hash>) -> Self {
        Self(hashes)
    }
}

impl Validate for RelationList {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        for hash in &self.0 {
            hash.validate()?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PinnedRelationList(Vec<GraphTips>);

impl PinnedRelationList {
    pub fn new(graph_tips_vec: Vec<GraphTips>) -> Self {
        Self(graph_tips_vec)
    }
}

impl Validate for PinnedRelationList {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // @TODO
        Ok(())
    }
}

/* #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Relation {
    /// Document id this relation is referring to.
    // @TODO: Replace inner value with DocumentId
    Unpinned(Hash),

    /// Reference to the exact version of the document.
    ///
    /// This field is `None` when there is no more than one operation (when the document only
    /// consists of one CREATE operation).
    Pinned(GraphTips),
}

impl Validate for Relation {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        match &self {
            Relation::Unpinned(hash) => {
                hash.validate()?;
            }
            Relation::Pinned(document_view) => {
                for operation_id in document_view {
                    operation_id.validate()?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RelationList {
    Unpinned(Vec<Hash>),
    Pinned(Vec<GraphTips>),
}

impl Validate for RelationList {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        match &self {
            RelationList::Unpinned(documents) => {
                for document in documents {
                    document.validate()?;
                }
            }
            RelationList::Pinned(document_view) => {
                for view in document_view {
                    for operation_id in view {
                        operation_id.validate()?;
                    }
                }
            }
        }

        Ok(())
    }
} */
