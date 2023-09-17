// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::slice::Iter;

use serde::{Deserialize, Serialize};

use crate::document::error::DocumentIdError;
use crate::document::{DocumentId, DocumentViewId};
use crate::operation::error::{
    PinnedRelationError, PinnedRelationListError, RelationError, RelationListError,
};
use crate::operation::OperationId;
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
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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

impl<'de> Deserialize<'de> for Relation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize into `DocumentId` struct
        let document_id: DocumentId = Deserialize::deserialize(deserializer)?;

        // Check format
        document_id
            .validate()
            .map_err(|err| serde::de::Error::custom(format!("invalid document id, {}", err)))?;

        Ok(Self(document_id))
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
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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

impl<'de> Deserialize<'de> for PinnedRelation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize into `DocumentViewId` struct
        let document_view_id: DocumentViewId = Deserialize::deserialize(deserializer)?;

        // Check format
        document_view_id.validate().map_err(|err| {
            serde::de::Error::custom(format!("invalid document view id, {}", err))
        })?;

        Ok(Self(document_view_id))
    }
}

/// A `RelationList` can be used to reference multiple foreign documents from a document field.
///
/// The item order and occurrences inside a relation list are defined by the developers and users
/// and have semantic meaning, for this reason we do not check against duplicates or ordering here.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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

impl<'de> Deserialize<'de> for RelationList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize into `DocumentId` array
        let document_ids: Vec<DocumentId> = Deserialize::deserialize(deserializer)?;

        // Convert and check format
        let relation_list = Self(document_ids);
        relation_list
            .validate()
            .map_err(|err| serde::de::Error::custom(format!("invalid document id, {}", err)))?;

        Ok(relation_list)
    }
}

/// A `PinnedRelationList` can be used to reference multiple documents views.
///
/// The item order and occurrences inside a pinned relation list are defined by the developers and
/// users and have semantic meaning, for this reason we do not check against duplicates or ordering
/// here.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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

impl<'de> Deserialize<'de> for PinnedRelationList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize into `DocumentViewId` array
        let document_view_ids: Vec<DocumentViewId> = Deserialize::deserialize(deserializer)?;

        // Convert and check format
        let pinned_relation_list = Self(document_view_ids);
        pinned_relation_list.validate().map_err(|err| {
            serde::de::Error::custom(format!("invalid document view id, {}", err))
        })?;

        Ok(pinned_relation_list)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use ciborium::cbor;
    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::hash::Hash;
    use crate::operation::OperationId;
    use crate::serde::{deserialize_into, serialize_from, serialize_value};
    use crate::test_utils::fixtures::random_document_id;
    use crate::test_utils::fixtures::random_hash;
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

        let pinned_relation_list = PinnedRelationList::new(vec![]);
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

    #[test]
    fn serialize_relation() {
        let hash_str = "0020b50b06774f909483c9c18e31b3bb17ff8f7d23088e9cc5a39260392259f34d42";
        let bytes = serialize_from(Relation::new(DocumentId::from_str(hash_str).unwrap()));
        assert_eq!(bytes, serialize_value(cbor!(hash_str)));
    }

    #[test]
    fn deserialize_relation() {
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let relation: Relation = deserialize_into(&serialize_value(cbor!(hash_str))).unwrap();
        assert_eq!(
            Relation::new(DocumentId::from_str(hash_str).unwrap()),
            relation
        );

        // Invalid hashes
        let invalid_hash = deserialize_into::<Relation>(&serialize_value(cbor!("1234")));
        assert!(invalid_hash.is_err());
        let empty_hash = deserialize_into::<Relation>(&serialize_value(cbor!("")));
        assert!(empty_hash.is_err());
    }

    #[test]
    fn serialize_pinned_relation() {
        let hash_str = "00208b050b24273b397f91a41e7f5030a853435dee0abbdc507dfc75a13809e7ba5f";
        let bytes = serialize_from(PinnedRelation::new(
            DocumentViewId::from_str(hash_str).unwrap(),
        ));
        assert_eq!(
            bytes,
            serialize_value(cbor!([[Hash::new(hash_str).unwrap()]]))
        );
    }

    #[test]
    fn deserialize_pinned_relation() {
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let pinned_relation: PinnedRelation = deserialize_into(&serialize_value(cbor!([
            "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805"
        ])))
        .unwrap();
        assert_eq!(
            PinnedRelation::new(DocumentViewId::from_str(hash_str).unwrap()),
            pinned_relation
        );

        // Invalid hashes
        let invalid_hash = deserialize_into::<PinnedRelation>(&serialize_value(cbor!(["1234"])));
        assert!(invalid_hash.is_err());
        let empty_hash = deserialize_into::<PinnedRelation>(&serialize_value(cbor!([])));
        assert!(empty_hash.is_err());

        // Invalid (non-canonic) order of operation ids
        let unordered = deserialize_into::<PinnedRelation>(&serialize_value(cbor!([
            "0020f1ab6d8114c0e7ab0af3bfd6862daf6ee0c510bbdf129e1780edfa505e860ff7",
            "0020a19353e7dfeb2f9031087c3428a2467bb684e25321f09298c64ce1a2fd5787d1",
        ])));
        assert!(unordered.is_err());

        // Duplicate operation ids
        let duplicate = deserialize_into::<PinnedRelation>(&serialize_value(cbor!([
            "05018634222cc8c9d49c5f48e8aecf0412c2cd2082a6712676373eaa1660e7af",
            "05018634222cc8c9d49c5f48e8aecf0412c2cd2082a6712676373eaa1660e7af",
        ])));
        assert!(duplicate.is_err());
    }

    #[test]
    fn serialize_relation_list() {
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let bytes = serialize_from(RelationList::new(vec![
            DocumentId::from_str(hash_str).unwrap()
        ]));
        assert_eq!(bytes, serialize_value(cbor!([hash_str])));
    }

    #[test]
    fn deserialize_relation_list() {
        let hash_str_1 = "0020deb1356bcdec02e05ce4f1fce51561bbfda68d1c4537c98c592b9e2bf9917122";
        let hash_str_2 = "002051044a3cfec6fea09759133dbae95dce9b49aa172df7fbb085c9b932694b2805";

        let relation_list: RelationList =
            deserialize_into(&serialize_value(cbor!([hash_str_1, hash_str_2]))).unwrap();
        assert_eq!(
            RelationList::new(vec![
                DocumentId::from_str(hash_str_1).unwrap(),
                DocumentId::from_str(hash_str_2).unwrap()
            ]),
            relation_list
        );

        // Invalid hash
        let invalid_hash = deserialize_into::<RelationList>(&serialize_value(cbor!(["1234"])));
        assert!(invalid_hash.is_err());
    }

    #[test]
    fn serialize_pinned_relation_list() {
        let hash_str_1 = "002051044a3cfec6fea09759133dbae95dce9b49aa172df7fbb085c9b932694b2805";
        let hash_str_2 = "0020deb1356bcdec02e05ce4f1fce51561bbfda68d1c4537c98c592b9e2bf9917122";
        let hash_str_3 = "002084d3c7eb7085c920879da6ea6c94cf89777e8f427a32f49d441fcda80cd39483";

        let bytes = serialize_from(PinnedRelationList::new(vec![
            DocumentViewId::new(&[
                OperationId::from_str(hash_str_1).unwrap(),
                OperationId::from_str(hash_str_2).unwrap(),
            ]),
            DocumentViewId::new(&[OperationId::from_str(hash_str_3).unwrap()]),
        ]));
        assert_eq!(
            bytes,
            serialize_value(cbor!([[hash_str_1, hash_str_2], [hash_str_3]]))
        );

        let bytes = serialize_from(PinnedRelationList::new(vec![]));
        assert_eq!(bytes, serialize_value(cbor!([])));
    }

    #[test]
    fn deserialize_pinned_relation_list() {
        let hash_str_1 = "002051044a3cfec6fea09759133dbae95dce9b49aa172df7fbb085c9b932694b2805";
        let hash_str_2 = "0020deb1356bcdec02e05ce4f1fce51561bbfda68d1c4537c98c592b9e2bf9917122";
        let hash_str_3 = "002084d3c7eb7085c920879da6ea6c94cf89777e8f427a32f49d441fcda80cd39483";

        let pinned_relation_list: PinnedRelationList = deserialize_into(&serialize_value(cbor!([
            [hash_str_1, hash_str_2],
            [hash_str_3]
        ])))
        .unwrap();
        assert_eq!(
            PinnedRelationList::new(vec![
                DocumentViewId::new(&[
                    OperationId::from_str(hash_str_1).unwrap(),
                    OperationId::from_str(hash_str_2).unwrap(),
                ]),
                DocumentViewId::new(&[OperationId::from_str(hash_str_3).unwrap()]),
            ]),
            pinned_relation_list
        );

        let pinned_relation_list: PinnedRelationList =
            deserialize_into(&serialize_value(cbor!([]))).unwrap();
        assert_eq!(PinnedRelationList::new(vec![]), pinned_relation_list);

        // Invalid hash
        let invalid_hash =
            deserialize_into::<PinnedRelationList>(&serialize_value(cbor!([["1234"]])));
        assert!(invalid_hash.is_err());

        // Invalid (non-canonic) order of operation ids
        let unordered = deserialize_into::<PinnedRelationList>(&serialize_value(cbor!([[
            "0020f1ab6d8114c0e7ab0af3bfd6862daf6ee0c510bbdf129e1780edfa505e860ff7",
            "0020a19353e7dfeb2f9031087c3428a2467bb684e25321f09298c64ce1a2fd5787d1",
        ]])));
        assert!(unordered.is_err());

        // Duplicate operation ids
        let duplicate = deserialize_into::<PinnedRelationList>(&serialize_value(cbor!([[
            "05018634222cc8c9d49c5f48e8aecf0412c2cd2082a6712676373eaa1660e7af",
            "05018634222cc8c9d49c5f48e8aecf0412c2cd2082a6712676373eaa1660e7af",
        ]])));
        assert!(duplicate.is_err());
    }
}
