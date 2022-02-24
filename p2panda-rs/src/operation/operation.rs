// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::btree_map::Iter;
use std::collections::BTreeMap;
use std::hash::{Hash as StdHash, Hasher};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::hash::Hash;
use crate::operation::{OperationEncoded, OperationError, OperationFieldsError};
use crate::Validate;

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

/// Operation format versions to introduce API changes in the future.
///
/// Operations contain the actual data of applications in the p2panda network and will be stored
/// for an indefinite time on different machines. To allow an upgrade path in the future and
/// support backwards compatibility for old data we can use this version number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[serde(untagged)]
#[repr(u8)]
pub enum OperationVersion {
    /// The default version number.
    Default = 1,
}

impl Copy for OperationVersion {}

/// Operations are categorised by their action type.
///
/// An action defines the operation format and if this operation creates, updates or deletes a data
/// document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationAction {
    /// Operation creates a new document.
    Create,

    /// Operation updates an existing document.
    Update,

    /// Operation deletes an existing document.
    Delete,
}

impl Serialize for OperationAction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match *self {
            OperationAction::Create => "create",
            OperationAction::Update => "update",
            OperationAction::Delete => "delete",
        })
    }
}

impl<'de> Deserialize<'de> for OperationAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        match s.as_str() {
            "create" => Ok(OperationAction::Create),
            "update" => Ok(OperationAction::Update),
            "delete" => Ok(OperationAction::Delete),
            _ => Err(serde::de::Error::custom("unknown operation action")),
        }
    }
}

impl Copy for OperationAction {}

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
    Relation(Relation),

    /// Reference to a list of documents.
    #[serde(rename = "relation_list")]
    RelationList(Vec<Relation>),
}

impl Validate for OperationValue {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        match self {
            Self::Relation(relation) => relation.validate(),
            Self::RelationList(relations) => {
                for relation in relations {
                    relation.validate()?;
                }

                Ok(())
            }
            _ => Ok(()),
        }
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

#[cfg_attr(doc, aquamarine::aquamarine)]
/// Operations describe data mutations of "documents" in the p2panda network. Authors send
/// operations to CREATE, UPDATE or DELETE documents.
///
/// The data itself lives in the "fields" object and is formed after an operation schema.
///
/// Starting from an initial CREATE operation, the following collection of UPDATE operations build
/// up a causal graph of mutations which can be resolved into a single object during a
/// "materialisation" process. If a DELETE operation is published it signals the deletion of the
/// entire graph and no more UPDATE operations should be published.
///
/// All UPDATE and DELETE operations have a `previous_operations` field which contains a vector of
/// operation hash ids which identify the known branch tips at the time of publication. These allow
/// us to build the graph and retain knowledge of the graph state at the time the specific
/// operation was published.
///
/// ## Examples
///
/// All of the examples are valid operation graphs. Operations which refer to more than one
/// previous operation help to reconcile branches. However, if other, unknown branches exist when
/// the graph is resolved, the materialisation process will still resolves the graph to a single
/// value.
///
/// 1)
/// ```mermaid
/// flowchart LR
///     A --- B --- C --- D;
///     B --- E --- F;
/// ```
///
/// 2)
/// ```mermaid
/// flowchart LR
///     B --- C --- D --- F;
///     A --- B --- E --- F;
/// ```
///
/// 3)
/// ```mermaid
/// flowchart LR
///     A --- B --- C;
///     A --- D --- E --- J;
///     B --- F --- G --- H --- I --- J;
/// ```
///
/// 4)
/// ```mermaid
/// flowchart LR
///     A --- B --- C --- D --- E;
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Operation {
    /// Describes if this operation creates, updates or deletes data.
    action: OperationAction,

    /// Hash of schema describing format of operation fields.
    schema: Hash,

    /// Version schema of this operation.
    version: OperationVersion,

    /// Optional array of hashes referring to operations directly preceding this one in the
    /// document.
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_operations: Option<Vec<Hash>>,

    /// Optional fields map holding the operation data.
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<OperationFields>,
}

impl Operation {
    /// Returns new CREATE operation.
    ///
    /// ## Example
    ///
    /// ```
    /// # extern crate p2panda_rs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use p2panda_rs::hash::Hash;
    /// use p2panda_rs::operation::{AsOperation, Operation, OperationFields, OperationValue};
    ///
    /// let schema_hash_string = "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";
    /// let schema_msg_hash = Hash::new(schema_hash_string)?;
    /// let mut msg_fields = OperationFields::new();
    ///
    /// msg_fields
    ///     .add(
    ///         "Zoo",
    ///         OperationValue::Text("Pandas, Doggos, Cats, and Parrots!".to_owned()),
    ///     )
    ///     .unwrap();
    ///
    /// let create_operation = Operation::new_create(schema_msg_hash, msg_fields)?;
    ///
    /// assert_eq!(AsOperation::is_create(&create_operation), true);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_create(schema: Hash, fields: OperationFields) -> Result<Self, OperationError> {
        let operation = Self {
            action: OperationAction::Create,
            version: OperationVersion::Default,
            schema,
            previous_operations: None,
            fields: Some(fields),
        };

        operation.validate()?;

        Ok(operation)
    }

    /// Returns new UPDATE operation.
    pub fn new_update(
        schema: Hash,
        previous_operations: Vec<Hash>,
        fields: OperationFields,
    ) -> Result<Self, OperationError> {
        let operation = Self {
            action: OperationAction::Update,
            version: OperationVersion::Default,
            schema,
            previous_operations: Some(previous_operations),
            fields: Some(fields),
        };

        operation.validate()?;

        Ok(operation)
    }

    /// Returns new DELETE operation.
    pub fn new_delete(
        schema: Hash,
        previous_operations: Vec<Hash>,
    ) -> Result<Self, OperationError> {
        let operation = Self {
            action: OperationAction::Delete,
            version: OperationVersion::Default,
            schema,
            previous_operations: Some(previous_operations),
            fields: None,
        };

        operation.validate()?;

        Ok(operation)
    }

    /// Encodes operation in CBOR format and returns bytes.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&self, &mut cbor_bytes).unwrap();
        cbor_bytes
    }
}

/// Shared methods for [`Operation`] and
/// [`OperationWithMeta`][crate::operation::OperationWithMeta].
pub trait AsOperation {
    /// Returns action type of operation.
    fn action(&self) -> OperationAction;

    /// Returns schema of operation.
    fn schema(&self) -> Hash;

    /// Returns version of operation.
    fn version(&self) -> OperationVersion;

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields>;

    /// Returns vector of known previous operation hashes of this operation.
    fn previous_operations(&self) -> Option<Vec<Hash>>;

    /// Returns true if operation contains fields.
    fn has_fields(&self) -> bool {
        self.fields().is_some()
    }

    /// Returns true if previous_operations contains a value.
    fn has_previous_operations(&self) -> bool {
        self.previous_operations().is_some()
    }

    /// Returns true when instance is CREATE operation.
    fn is_create(&self) -> bool {
        self.action() == OperationAction::Create
    }

    /// Returns true when instance is UPDATE operation.
    fn is_update(&self) -> bool {
        self.action() == OperationAction::Update
    }

    /// Returns true when instance is DELETE operation.
    fn is_delete(&self) -> bool {
        self.action() == OperationAction::Delete
    }
}

impl AsOperation for Operation {
    /// Returns action type of operation.
    fn action(&self) -> OperationAction {
        self.action.to_owned()
    }

    /// Returns version of operation.
    fn version(&self) -> OperationVersion {
        self.version.to_owned()
    }

    /// Returns schema of operation.
    fn schema(&self) -> Hash {
        self.schema.to_owned()
    }

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        self.fields.clone()
    }

    /// Returns known previous operations vector of this operation.
    fn previous_operations(&self) -> Option<Vec<Hash>> {
        self.previous_operations.clone()
    }
}

/// Decodes an encoded operation and returns it.
impl From<&OperationEncoded> for Operation {
    fn from(operation_encoded: &OperationEncoded) -> Self {
        ciborium::de::from_reader(&operation_encoded.to_bytes()[..]).unwrap()
    }
}

impl PartialEq for Operation {
    fn eq(&self, other: &Self) -> bool {
        self.to_cbor() == other.to_cbor()
    }
}

impl Eq for Operation {}

impl StdHash for Operation {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_cbor().hash(state);
    }
}

impl Validate for Operation {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // CREATE and UPDATE operations can not have empty fields.
        if !self.is_delete() && (!self.has_fields() || self.fields().unwrap().is_empty()) {
            return Err(OperationError::EmptyFields);
        }

        // DELETE must have empty fields
        if self.is_delete() && self.has_fields() {
            return Err(OperationError::DeleteWithFields);
        }

        // UPDATE and DELETE operations must contain previous_operations.
        if !self.is_create() && (!self.has_previous_operations()) {
            return Err(OperationError::EmptyPreviousOperations);
        }

        // CREATE operations must not contain previous_operations.
        if self.is_create() && (self.has_previous_operations()) {
            return Err(OperationError::ExistingPreviousOperations);
        }

        // Validate fields
        if self.has_fields() {
            self.fields().unwrap().validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::convert::TryFrom;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::hash::Hash;
    use crate::operation::OperationEncoded;
    use crate::test_utils::fixtures::templates::many_valid_operations;
    use crate::test_utils::fixtures::{fields, random_hash, schema};
    use crate::Validate;

    use super::{
        AsOperation, Operation, OperationAction, OperationFields, OperationValue, OperationVersion,
        Relation,
    };

    #[test]
    fn operation_fields() {
        let mut fields = OperationFields::new();

        // Detect duplicate
        fields
            .add("test", OperationValue::Text("Hello, Panda!".to_owned()))
            .unwrap();

        assert!(fields
            .add("test", OperationValue::Text("Huhu".to_owned()))
            .is_err());

        // Bail when key does not exist
        assert!(fields
            .update("imagine", OperationValue::Text("Pandaparty".to_owned()))
            .is_err());
    }

    #[rstest]
    fn relation_lists(
        #[from(random_hash)] document_1: Hash,
        #[from(random_hash)] document_2: Hash,
        #[from(random_hash)] operation_id_1: Hash,
        #[from(random_hash)] operation_id_2: Hash,
        #[from(random_hash)] operation_id_3: Hash,
    ) {
        let document_view_1 = vec![operation_id_1, operation_id_2];
        let document_view_2 = vec![operation_id_3];

        let mut relations: Vec<Relation> = Vec::new();
        relations.push(Relation::new(document_1, document_view_1));
        relations.push(Relation::new(document_2, document_view_2));

        let mut fields = OperationFields::new();
        fields
            .add("locations", OperationValue::RelationList(relations))
            .unwrap();
    }

    #[rstest]
    fn operation_validation(
        fields: OperationFields,
        schema: Hash,
        #[from(random_hash)] prev_op_id: Hash,
    ) {
        let invalid_create_operation_1 = Operation {
            action: OperationAction::Create,
            version: OperationVersion::Default,
            schema: schema.clone(),
            previous_operations: None,
            // CREATE operations must contain fields
            fields: None, // Error
        };

        assert!(invalid_create_operation_1.validate().is_err());

        let invalid_create_operation_2 = Operation {
            action: OperationAction::Create,
            version: OperationVersion::Default,
            schema: schema.clone(),
            // CREATE operations must not contain previous_operations
            previous_operations: Some(vec![prev_op_id.clone()]), // Error
            fields: Some(fields.clone()),
        };

        assert!(invalid_create_operation_2.validate().is_err());

        let invalid_update_operation_1 = Operation {
            action: OperationAction::Update,
            version: OperationVersion::Default,
            schema: schema.clone(),
            // UPDATE operations must contain previous_operations
            previous_operations: None, // Error
            fields: Some(fields.clone()),
        };

        assert!(invalid_update_operation_1.validate().is_err());

        let invalid_update_operation_2 = Operation {
            action: OperationAction::Update,
            version: OperationVersion::Default,
            schema: schema.clone(),
            previous_operations: Some(vec![prev_op_id.clone()]),
            // UPDATE operations must contain fields
            fields: None, // Error
        };

        assert!(invalid_update_operation_2.validate().is_err());

        let invalid_delete_operation_1 = Operation {
            action: OperationAction::Delete,
            version: OperationVersion::Default,
            schema: schema.clone(),
            // DELETE operations must contain previous_operations
            previous_operations: None, // Error
            fields: None,
        };

        assert!(invalid_delete_operation_1.validate().is_err());

        let invalid_delete_operation_2 = Operation {
            action: OperationAction::Delete,
            version: OperationVersion::Default,
            schema,
            previous_operations: Some(vec![prev_op_id]),
            // DELETE operations must not contain fields
            fields: Some(fields), // Error
        };

        assert!(invalid_delete_operation_2.validate().is_err())
    }

    #[rstest]
    fn encode_and_decode(
        schema: Hash,
        #[from(random_hash)] prev_op_id: Hash,
        #[from(random_hash)] document: Hash,
    ) {
        // Create test operation
        let mut fields = OperationFields::new();

        // Add one field for every kind of OperationValue
        fields
            .add("username", OperationValue::Text("bubu".to_owned()))
            .unwrap();

        fields.add("height", OperationValue::Float(3.5)).unwrap();

        fields.add("age", OperationValue::Integer(28)).unwrap();

        fields
            .add("is_admin", OperationValue::Boolean(false))
            .unwrap();

        fields
            .add(
                "profile_picture",
                OperationValue::Relation(Relation::new(document, Vec::new())),
            )
            .unwrap();

        let operation = Operation::new_update(schema, vec![prev_op_id], fields).unwrap();

        assert!(operation.is_update());

        // Encode operation ...
        let encoded = OperationEncoded::try_from(&operation).unwrap();

        // ... and decode it again
        let operation_restored = Operation::try_from(&encoded).unwrap();

        assert_eq!(operation, operation_restored);
    }

    #[rstest]
    fn field_ordering(schema: Hash) {
        // Create first test operation
        let mut fields = OperationFields::new();
        fields
            .add("a", OperationValue::Text("sloth".to_owned()))
            .unwrap();
        fields
            .add("b", OperationValue::Text("penguin".to_owned()))
            .unwrap();

        let first_operation = Operation::new_create(schema.clone(), fields).unwrap();

        // Create second test operation with same values but different order of fields
        let mut second_fields = OperationFields::new();
        second_fields
            .add("b", OperationValue::Text("penguin".to_owned()))
            .unwrap();
        second_fields
            .add("a", OperationValue::Text("sloth".to_owned()))
            .unwrap();

        let second_operation = Operation::new_create(schema, second_fields).unwrap();

        assert_eq!(first_operation.to_cbor(), second_operation.to_cbor());
    }

    #[test]
    fn field_iteration() {
        // Create first test operation
        let mut fields = OperationFields::new();
        fields
            .add("a", OperationValue::Text("sloth".to_owned()))
            .unwrap();
        fields
            .add("b", OperationValue::Text("penguin".to_owned()))
            .unwrap();

        let mut field_iterator = fields.iter();

        assert_eq!(
            field_iterator.next().unwrap().1,
            &OperationValue::Text("sloth".to_owned())
        );
        assert_eq!(
            field_iterator.next().unwrap().1,
            &OperationValue::Text("penguin".to_owned())
        );
    }

    #[apply(many_valid_operations)]
    fn many_valid_operations_should_encode(#[case] operation: Operation) {
        assert!(OperationEncoded::try_from(&operation).is_ok())
    }

    #[apply(many_valid_operations)]
    fn it_hashes(#[case] operation: Operation) {
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&operation, key_value.clone());
        let key_value_retrieved = hash_map.get(&operation).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }
}
