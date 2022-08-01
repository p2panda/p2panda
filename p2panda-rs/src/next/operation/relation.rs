// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::slice::Iter;

use crate::next::document::error::DocumentIdError;
use crate::next::document::{DocumentId, DocumentViewId};
use crate::next::operation::error::{
    PinnedRelationError, PinnedRelationListError, RelationError, RelationListError,
};
use crate::next::operation::OperationId;
use crate::Validate;

/// Field type representing references to other documents.
///
/// Relation types describe references to other documents.
///
/// Similar to SQL relationships, documents refer to one another by their _document id_. This module
/// provides types used in operations to refer to one (`Relation`) or many documents
/// (`RelationList`).
///
/// This is an example of a simple `Relation` where a _Comment_ Document refers to a _Blog Post_
/// Document:
///
/// ```text
/// Document: [Blog-Post "Monday evening"]
///     ^
///     |
/// Document: [Comment "This was great!"]
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
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
    type Error = RelationError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()?;
        Ok(())
    }
}

/// Reference to the exact version of the document.
///
/// `PinnedRelation` _pin_ a relation to a specific, immutable version of a document or many
/// documents when necessary (`PinnedRelation` or `PinnedRelationList`).
///
/// When the blog post from the `Relation` example changes its contents from _Monday evening_ to
/// _Tuesday morning_ the comment would automatically refer to the new version as the comment
/// refers to the document as a whole, including all future changes.
///
/// Since the comment was probably meant to be referring to Monday when it was created, we have to
/// _pin_ it to the exact version of the blog post in order to preserve this meaning. A
/// `PinnedRelation` achieves this by referring to the blog post's _document view id_:
///
/// ```text
///                    Document-View                              Document-View
///                         |                                           |
/// Document: [Blog-Post "Monday evening"] -- UPDATE -- > [Blog-Post "Tuesday morning"]
///                   ^
///                   |
///      _____________|  Pinned Relation (we will stay in the "past")
///     |
///     |
/// Document: [Comment "This was great!"]
/// ```
///
/// Document view ids contain the operation ids of the document graph tips, which is all the
/// information we need to reliably recreate the document at this certain point in time.
///
/// Pinned relations give us immutability and the option to restore a historical state across
/// documents. However, most cases will probably only need unpinned relations: For example when
/// referring to a user-profile you probably want to always get the _latest_ version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinnedRelation(DocumentViewId);

impl PinnedRelation {
    /// Returns a new pinned relation field.
    pub fn new(document_view_id: DocumentViewId) -> Self {
        Self(document_view_id)
    }

    /// Returns the pinned relation's document view id.
    pub fn view_id(&self) -> &DocumentViewId {
        &self.0
    }

    /// Returns iterator over operation ids.
    pub fn iter(&self) -> Iter<OperationId> {
        self.0.iter()
    }
}

impl Validate for PinnedRelation {
    type Error = PinnedRelationError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()?;
        Ok(())
    }
}

/// A `RelationList` can be used to reference multiple foreign documents from a document field.
///
/// The item order and occurrences inside a relation list are defined by the developers and users
/// and have semantic meaning, for this reason we do not check against duplicates or ordering here.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::len_without_is_empty)]
pub struct RelationList(Vec<DocumentId>);

impl RelationList {
    /// Returns a new list of relations.
    pub fn new(relations: Vec<DocumentId>) -> Self {
        Self(relations)
    }

    /// Returns the list of document ids.
    pub fn document_ids(&self) -> &[DocumentId] {
        self.0.as_slice()
    }

    /// Returns iterator over document ids.
    pub fn iter(&self) -> Iter<DocumentId> {
        self.0.iter()
    }

    /// Returns number of documents in this relation list.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Validate for RelationList {
    type Error = RelationListError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Note that we do NOT check for duplicates and ordering here as this information is
        // semantic!
        for document_id in &self.0 {
            document_id.validate()?;
        }

        Ok(())
    }
}

impl TryFrom<&[String]> for RelationList {
    type Error = RelationListError;

    fn try_from(str_list: &[String]) -> Result<Self, Self::Error> {
        let document_ids: Result<Vec<DocumentId>, DocumentIdError> = str_list
            .iter()
            .map(|document_id_str| document_id_str.parse::<DocumentId>())
            .collect();

        Ok(Self(document_ids?))
    }
}

/// A `PinnedRelationList` can be used to reference multiple documents views.
///
/// The item order and occurrences inside a pinned relation list are defined by the developers and
/// users and have semantic meaning, for this reason we do not check against duplicates or ordering
/// here.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::len_without_is_empty)]
pub struct PinnedRelationList(Vec<DocumentViewId>);

impl PinnedRelationList {
    /// Returns a new list of pinned relations.
    pub fn new(relations: Vec<DocumentViewId>) -> Self {
        Self(relations)
    }

    /// Returns the list of document view ids.
    pub fn document_view_ids(&self) -> &[DocumentViewId] {
        self.0.as_slice()
    }

    /// Returns iterator over document view ids.
    pub fn iter(&self) -> Iter<DocumentViewId> {
        self.0.iter()
    }

    /// Returns number of pinned documents in this list.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Validate for PinnedRelationList {
    type Error = PinnedRelationListError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Note that we do NOT check for duplicates and ordering here as this information is
        // semantic!
        for document_view_id in &self.0 {
            document_view_id.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::next::document::{DocumentId, DocumentViewId};
    use crate::next::hash::Hash;
    use crate::next::test_utils::fixtures::random_document_id;
    use crate::next::test_utils::fixtures::random_hash;
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

        let pinned_relation = PinnedRelation::new(DocumentViewId::from(operation_id_1.clone()));
        assert!(pinned_relation.validate().is_ok());

        let relation_list = RelationList::new(vec![document_1, document_2]);
        assert!(relation_list.validate().is_ok());

        let pinned_relation_list =
            PinnedRelationList::new(vec![operation_id_1.into(), operation_id_2.into()]);
        assert!(pinned_relation_list.validate().is_ok());
    }

    #[rstest]
    fn iterates(#[from(random_hash)] hash_1: Hash, #[from(random_hash)] hash_2: Hash) {
        let pinned_relation = PinnedRelation::new(DocumentViewId::new(&[
            hash_1.clone().into(),
            hash_2.clone().into(),
        ]));

        for hash in pinned_relation.iter() {
            assert!(hash.validate().is_ok());
        }

        let relation_list = RelationList::new(vec![
            DocumentId::new(&hash_1.clone().into()),
            DocumentId::new(&hash_2.clone().into()),
        ]);

        for document_id in relation_list.iter() {
            assert!(document_id.validate().is_ok());
        }

        let pinned_relation_list = PinnedRelationList::new(vec![
            DocumentViewId::from(hash_1),
            DocumentViewId::from(hash_2),
        ]);

        for pinned_relation in pinned_relation_list.iter() {
            for hash in pinned_relation.graph_tips() {
                assert!(hash.validate().is_ok());
            }
        }
    }

    #[rstest]
    fn list_equality(
        #[from(random_document_id)] document_1: DocumentId,
        #[from(random_document_id)] document_2: DocumentId,
        #[from(random_hash)] operation_id_1: Hash,
        #[from(random_hash)] operation_id_2: Hash,
    ) {
        let relation_list = RelationList::new(vec![document_1.clone(), document_2.clone()]);
        let relation_list_different_order = RelationList::new(vec![document_2, document_1]);
        assert_ne!(relation_list, relation_list_different_order);

        let pinned_relation_list = PinnedRelationList::new(vec![
            operation_id_1.clone().into(),
            operation_id_2.clone().into(),
        ]);
        let pinned_relation_list_different_order =
            PinnedRelationList::new(vec![operation_id_2.into(), operation_id_1.into()]);
        assert_ne!(pinned_relation_list, pinned_relation_list_different_order);
    }
}
