// SPDX-License-Identifier: AGPL-3.0-or-later

//! Relation types describe references to other documents.
//!
//! Similar to SQL relationships, documents refer to one another by their _document id_. This module
//! provides types used in operations to refer to one (`Relation`) or many documents
//! (`RelationList`).
//!
//! This is an example of a simple `Relation` where a _Comment_ Document refers to a _Blog Post_
//! Document:
//!
//! ```text
//! Document: [Blog-Post "Monday evening"]
//!     ^
//!     |
//! Document: [Comment "This was great!"]
//! ```
//!
//! ## Pinned relations
//!
//! Relations can optionally be _pinned_ to a specific, immutable version of a document or many
//! documents when necessary (`PinnedRelation` or `PinnedRelationList`).
//!
//! When the blog post from the example above changes its contents from _Monday evening_ to
//! _Tuesday morning_ the comment would automatically refer to the new version as the comment
//! refers to the document as a whole, including all future changes.
//!
//! Since the comment was probably meant to be referring to Monday when it was created, we have to
//! _pin_ it to the exact version of the blog post in order to preserve this meaning. A
//! `PinnedRelation` achieves this by referring to the blog post's _document view id_:
//!
//! ```text
//!                    Document-View                              Document-View
//!                         |                                           |
//! Document: [Blog-Post "Monday evening"] -- UPDATE -- > [Blog-Post "Tuesday morning"]
//!                   ^
//!                   |
//!      _____________|  Pinned Relation (we will stay in the "past")
//!     |
//!     |
//! Document: [Comment "This was great!"]
//! ```
//!
//! Document view ids contain the hashes of the document graph tips, which is all the information
//! we need to reliably recreate the document at this certain point in time.
//!
//! Pinned relations give us immutability and the option to restore a historical state across
//! documents. However, most cases will probably only need unpinned relations: For example when
//! referring to a user-profile you probably want to always get the _latest_ version.
use serde::{Deserialize, Serialize};

use crate::document::{DocumentId, DocumentViewId};
use crate::hash::{Hash, HashError};
use crate::Validate;

/// Field type representing references to other documents.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Relation(DocumentId);

impl Relation {
    /// Returns a new relation field.
    pub fn new(document: DocumentId) -> Self {
        Self(document)
    }

    /// Returns the relations document id.
    pub fn document_id(&self) -> &DocumentId {
        &self.0
    }
}

impl Validate for Relation {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()
    }
}

/// Reference to the exact version of the document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PinnedRelation(DocumentViewId);

impl PinnedRelation {
    /// Returns a new pinned relation field.
    pub fn new(document_view_id: DocumentViewId) -> Self {
        Self(document_view_id)
    }
}

impl Validate for PinnedRelation {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()
    }
}

impl IntoIterator for PinnedRelation {
    type Item = Hash;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A `RelationList` can be used to reference multiple foreign documents from a document field.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelationList(Vec<DocumentId>);

impl RelationList {
    /// Returns a new list of relations.
    pub fn new(relations: Vec<DocumentId>) -> Self {
        Self(relations)
    }

    /// Returns an iterator over the `DocumentId`s in this `RelationList`
    pub fn iter(&self) -> std::vec::IntoIter<DocumentId> {
        self.0.clone().into_iter()
    }
}

impl Validate for RelationList {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        for document_id in &self.0 {
            document_id.validate()?;
        }

        Ok(())
    }
}

impl IntoIterator for RelationList {
    type Item = DocumentId;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A `PinnedRelationList` can be used to reference multiple documents views.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PinnedRelationList(Vec<DocumentViewId>);

impl PinnedRelationList {
    /// Returns a new list of pinned relations.
    pub fn new(relations: Vec<DocumentViewId>) -> Self {
        Self(relations)
    }

    /// Returns an iterator over the `DocumentViewId`s in this `PinnedRelationList`
    pub fn iter(&self) -> std::vec::IntoIter<DocumentViewId> {
        self.0.clone().into_iter()
    }
}

impl Validate for PinnedRelationList {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        for document_view in &self.0 {
            document_view.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::hash::Hash;
    use crate::test_utils::fixtures::{random_document_id, random_hash};
    use crate::Validate;

    use super::{PinnedRelation, PinnedRelationList, Relation, RelationList};

    #[rstest]
    fn validation(
        #[from(random_document_id)] document_1: DocumentId,
        #[from(random_document_id)] document_2: DocumentId,
        #[from(random_hash)] operation_id_1: Hash,
        #[from(random_hash)] operation_id_2: Hash,
    ) {
        let relation = Relation::new(document_1.clone());
        assert!(relation.validate().is_ok());

        let pinned_relation =
            PinnedRelation::new(DocumentViewId::new(vec![operation_id_1.clone()]));
        assert!(pinned_relation.validate().is_ok());

        let relation_list = RelationList::new(vec![document_1, document_2]);
        assert!(relation_list.validate().is_ok());

        let pinned_relation_list = PinnedRelationList::new(vec![
            DocumentViewId::new(vec![operation_id_1]),
            DocumentViewId::new(vec![operation_id_2]),
        ]);
        assert!(pinned_relation_list.validate().is_ok());
    }
}
