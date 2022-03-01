// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::btree_map::Iter;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::hash::HashError;
use crate::operation::{
    OperationError, OperationFieldsError, PinnedRelation, PinnedRelationList, Relation,
    RelationList,
};
use crate::Validate;

/// Wrapper around relation types which can be both pinned and unpinned.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OperationValueRelation {
    /// Value refers to a document.
    Unpinned(Relation),

    /// Value refers to a "pinned" document at a certain point in time.
    Pinned(PinnedRelation),
}

impl Validate for OperationValueRelation {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        match self {
            Self::Unpinned(relation) => relation.validate(),
            Self::Pinned(relation) => relation.validate(),
        }
    }
}

/// Wrapper around relation list types which can be both pinned and unpinned.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OperationValueRelationList {
    /// Value refers to a list of documents.
    Unpinned(RelationList),

    /// Value refers to a list of "pinned" documents from a certain point in time.
    Pinned(PinnedRelationList),
}

impl Validate for OperationValueRelationList {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        match self {
            Self::Unpinned(relation_list) => relation_list.validate(),
            Self::Pinned(relation_list) => relation_list.validate(),
        }
    }
}

/// Enum of possible data types which can be added to the operations fields as values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum OperationValue {
    /// Boolean value.
    #[serde(rename = "bool")]
    Boolean(bool),

    /// Signed integer value.
    #[serde(rename = "int")]
    Integer(i64),

    /// Floating point value.
    #[serde(rename = "float")]
    Float(f64),

    /// String value.
    #[serde(rename = "str")]
    Text(String),

    /// Reference to a document.
    #[serde(rename = "relation")]
    Relation(OperationValueRelation),

    /// Reference to a list of documents.
    #[serde(rename = "relation_list")]
    RelationList(OperationValueRelationList),
}

impl Validate for OperationValue {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        match self {
            Self::Relation(relation) => relation.validate(),
            Self::RelationList(relations) => relations.validate(),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
// Methods only used for testing of (invalid) operation values.
impl OperationValue {
    /// Encodes an operation value encoded and returns CBOR hex string.
    pub(super) fn serialize(&self) -> String {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&self, &mut cbor_bytes).unwrap();
        hex::encode(cbor_bytes)
    }

    /// Decodes an operation value encoded as CBOR hex string and returns it.
    pub(super) fn deserialize_str(str: &str) -> Self {
        let bytes = hex::decode(str).unwrap();
        ciborium::de::from_reader(&bytes[..]).unwrap()
    }
}

/// Operation fields are used to store application data. They are implemented as a simple key/value
/// store with support for a limited number of data types (see [`OperationValue`] for further
/// documentation on this). A `OperationFields` instance can contain any number and types of
/// fields. However, when a `OperationFields` instance is attached to a `Operation`, the
/// operation's schema determines which fields may be used.
///
/// Internally operation fields use sorted B-Tree maps to assure ordering of the fields. If the
/// operation fields would not be sorted consistently we would get different hash results for the
/// same contents.
///
/// # Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> () {
/// # use p2panda_rs::operation::{OperationFields, OperationValue, AsOperation};
/// let mut fields = OperationFields::new();
/// fields
///     .add("title", OperationValue::Text("Hello, Panda!".to_owned()))
///     .unwrap();
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct OperationFields(BTreeMap<String, OperationValue>);

impl OperationFields {
    /// Creates a new fields instance to add data to.
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns the number of added fields.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true when no field is given.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Adds a new field to this instance.
    ///
    /// A field is a simple key/value pair.
    pub fn add(&mut self, name: &str, value: OperationValue) -> Result<(), OperationFieldsError> {
        if self.0.contains_key(name) {
            return Err(OperationFieldsError::FieldDuplicate);
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    /// Overwrites an already existing field with a new value.
    pub fn update(
        &mut self,
        name: &str,
        value: OperationValue,
    ) -> Result<(), OperationFieldsError> {
        if !self.0.contains_key(name) {
            return Err(OperationFieldsError::UnknownField);
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    /// Removes an existing field from this instance.
    pub fn remove(&mut self, name: &str) -> Result<(), OperationFieldsError> {
        if !self.0.contains_key(name) {
            return Err(OperationFieldsError::UnknownField);
        }

        self.0.remove(name);

        Ok(())
    }

    /// Returns a field value.
    pub fn get(&self, name: &str) -> Option<&OperationValue> {
        if !self.0.contains_key(name) {
            return None;
        }

        self.0.get(name)
    }

    /// Returns an array of existing operation keys.
    pub fn keys(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    /// Returns an iterator of existing operation fields.
    pub fn iter(&self) -> Iter<String, OperationValue> {
        self.0.iter()
    }
}

impl Validate for OperationFields {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        for (_, value) in self.iter() {
            value.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::hash::Hash;
    use crate::operation::{PinnedRelation, PinnedRelationList, Relation, RelationList};
    use crate::test_utils::fixtures::{random_document_id, random_hash};
    use crate::Validate;

    use super::{
        OperationFields, OperationValue, OperationValueRelation, OperationValueRelationList,
    };

    #[test]
    fn operation_fields() {
        let mut fields = OperationFields::new();

        // Detect duplicate
        fields
            .add("message", OperationValue::Text("Hello, Panda!".to_owned()))
            .unwrap();

        // Have to use `update` to change fields
        assert!(fields
            .add("message", OperationValue::Text("Huhu".to_owned()))
            .is_err());

        assert!(fields
            .update("message", OperationValue::Text("Huhu".to_owned()))
            .is_ok());

        // Bail when key does not exist
        assert!(fields
            .update("imagine", OperationValue::Text("Pandaparty".to_owned()))
            .is_err());

        assert_eq!(fields.keys(), vec!["message"]);

        assert!(fields.remove("message").is_ok());

        assert_eq!(fields.len(), 0);
    }

    #[rstest]
    fn encode_decode_relations(
        #[from(random_hash)] hash_1: Hash,
        #[from(random_hash)] hash_2: Hash,
        #[from(random_hash)] hash_3: Hash,
        #[from(random_hash)] hash_4: Hash,
        #[from(random_hash)] hash_5: Hash,
        #[from(random_hash)] hash_6: Hash,
        #[from(random_hash)] hash_7: Hash,
        #[from(random_hash)] hash_8: Hash,
    ) {
        // 1. Unpinned relation
        let relation = OperationValue::Relation(OperationValueRelation::Unpinned(Relation::new(
            DocumentId::new(hash_1),
        )));
        assert_eq!(
            relation,
            OperationValue::deserialize_str(&relation.serialize())
        );

        // 2. Pinned relation
        let pinned_relation = OperationValue::Relation(OperationValueRelation::Pinned(
            PinnedRelation::new(DocumentViewId::new(vec![hash_2, hash_3])),
        ));
        assert_eq!(
            pinned_relation,
            OperationValue::deserialize_str(&pinned_relation.serialize())
        );

        // 3. Unpinned relation list
        let relation_list = OperationValue::RelationList(OperationValueRelationList::Unpinned(
            RelationList::new(vec![DocumentId::new(hash_4), DocumentId::new(hash_5)]),
        ));
        assert_eq!(
            relation_list,
            OperationValue::deserialize_str(&relation_list.serialize())
        );

        // 4. Pinned relation list
        let pinned_relation_list = OperationValue::RelationList(
            OperationValueRelationList::Pinned(PinnedRelationList::new(vec![
                DocumentViewId::new(vec![hash_6, hash_7]),
                DocumentViewId::new(vec![hash_8]),
            ])),
        );
        assert_eq!(
            pinned_relation_list,
            OperationValue::deserialize_str(&pinned_relation_list.serialize())
        );
    }

    #[rstest]
    fn validation_ok(
        #[from(random_document_id)] document_1: DocumentId,
        #[from(random_document_id)] document_2: DocumentId,
        #[from(random_hash)] operation_id_1: Hash,
        #[from(random_hash)] operation_id_2: Hash,
    ) {
        let relation = Relation::new(document_1.clone());
        let value = OperationValue::Relation(OperationValueRelation::Unpinned(relation));
        assert!(value.validate().is_ok());

        let pinned_relation = PinnedRelation::new(DocumentViewId::new(vec![
            operation_id_1.clone(),
            operation_id_2.clone(),
        ]));
        let value = OperationValue::Relation(OperationValueRelation::Pinned(pinned_relation));
        assert!(value.validate().is_ok());

        let relation_list = RelationList::new(vec![document_1, document_2]);
        let value =
            OperationValue::RelationList(OperationValueRelationList::Unpinned(relation_list));
        assert!(value.validate().is_ok());

        let pinned_relation_list = PinnedRelationList::new(vec![
            DocumentViewId::new(vec![operation_id_1]),
            DocumentViewId::new(vec![operation_id_2]),
        ]);
        let value =
            OperationValue::RelationList(OperationValueRelationList::Pinned(pinned_relation_list));
        assert!(value.validate().is_ok());
    }

    #[test]
    fn validation_invalid_relations() {
        // "relation_list" operation value with invalid hash:
        //
        // {
        //  "type": "relation_list",
        //  "value": ["This is not a hash"]
        // }
        let invalid_hash = "A264747970656D72656C6174696F6E5F6C6973746576616C7565817254686973206973206E6F7420612068617368";
        let value: OperationValue = OperationValue::deserialize_str(invalid_hash);
        assert!(value.validate().is_err());

        // "relation" operation value with invalid hash:
        //
        // {
        //  "type": "relation",
        //  "value": "This is not a hash"
        // }
        let invalid_hash =
            "A264747970656872656C6174696F6E6576616C75657254686973206973206E6F7420612068617368";
        let value: OperationValue = OperationValue::deserialize_str(invalid_hash);
        assert!(value.validate().is_err());
    }

    #[test]
    fn validation_relation_lists_can_be_empty() {
        let pinned_relation_list = PinnedRelationList::new(vec![]);
        let value =
            OperationValue::RelationList(OperationValueRelationList::Pinned(pinned_relation_list));
        assert!(value.validate().is_ok());

        let relation_list = RelationList::new(vec![]);
        let value =
            OperationValue::RelationList(OperationValueRelationList::Unpinned(relation_list));
        assert!(value.validate().is_ok());
    }

    #[rstest]
    fn relation_lists(
        #[from(random_document_id)] document_1: DocumentId,
        #[from(random_document_id)] document_2: DocumentId,
    ) {
        let relations = RelationList::new(vec![document_1, document_2]);
        let value = OperationValue::RelationList(OperationValueRelationList::Unpinned(relations));
        let mut fields = OperationFields::new();
        assert!(fields.add("locations", value).is_ok());
    }

    #[rstest]
    fn pinned_relation_lists(
        #[from(random_hash)] operation_id_1: Hash,
        #[from(random_hash)] operation_id_2: Hash,
        #[from(random_hash)] operation_id_3: Hash,
        #[from(random_hash)] operation_id_4: Hash,
        #[from(random_hash)] operation_id_5: Hash,
        #[from(random_hash)] operation_id_6: Hash,
    ) {
        let document_view_id_1 = DocumentViewId::new(vec![operation_id_1, operation_id_2]);
        let document_view_id_2 = DocumentViewId::new(vec![operation_id_3]);
        let document_view_id_3 =
            DocumentViewId::new(vec![operation_id_4, operation_id_5, operation_id_6]);

        let relations = PinnedRelationList::new(vec![
            document_view_id_1,
            document_view_id_2,
            document_view_id_3,
        ]);

        let value = OperationValue::RelationList(OperationValueRelationList::Pinned(relations));
        let mut fields = OperationFields::new();
        assert!(fields.add("locations", value).is_ok());
    }
}
