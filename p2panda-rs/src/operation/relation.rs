// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::operation::OperationError;
use crate::Validate;

/// A `RelationList` can be used to reference multiple foreign documents from a document field.
pub type RelationList = Vec<Relation>;

/// Field type representing references to other documents.
///
/// The "relation" field type references a document id and the historical state which it had at the
/// point this relation was created.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    /// Document id this relation is referring to.
    document: Hash,

    /// Reference to the exact version of the document.
    ///
    /// This field is `None` when there is no more than one operation (when the document only
    /// consists of one CREATE operation).
    #[serde(skip_serializing_if = "Option::is_none")]
    document_view: Option<Vec<Hash>>,
}

impl Relation {
    /// Returns a new relation field type.
    pub fn new(document: Hash, document_view: Vec<Hash>) -> Self {
        Self {
            document,
            document_view: match document_view.is_empty() {
                true => None,
                false => Some(document_view),
            },
        }
    }

    /// Returns the relations document id
    pub fn document_id(&self) -> &Hash {
        &self.document
    }
}

impl Validate for Relation {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.document.validate()?;

        match &self.document_view {
            Some(view) => {
                for operation_id in view {
                    operation_id.validate()?;
                }

                Ok(())
            }
            None => Ok(()),
        }
    }
}
