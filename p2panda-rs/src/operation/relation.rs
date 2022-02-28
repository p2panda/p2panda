// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::operation::OperationError;
use crate::Validate;

// @TODO: Replace this with DocumentViewId
type GraphTips = Vec<Hash>;

/// Field type representing references to other documents.
///
/// The "relation" field type references a document id and the historical state which it had at the
/// point this relation was created.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
}
