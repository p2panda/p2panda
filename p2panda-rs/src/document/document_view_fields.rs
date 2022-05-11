// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::btree_map::Iter;
use std::collections::BTreeMap;

use crate::operation::{OperationId, OperationValue};

/// A key value map of field keys to DocumentViewValues.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentViewFields(BTreeMap<String, DocumentViewValue>);

/// An enum encapsulating the current value of a document fiew field as well as the id of
/// the operation it came from.
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentViewValue {
    Value(OperationId, OperationValue),
    Deleted(OperationId),
}

impl DocumentViewValue {
    pub fn id(&self) -> &OperationId {
        match self {
            DocumentViewValue::Value(id, _) => &id,
            DocumentViewValue::Deleted(id) => &id,
        }
    }
}

impl DocumentViewFields {
    /// Creates a new fields instance to add data to.
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Insert a new field to this instance.
    pub fn insert(&mut self, name: &str, value: DocumentViewValue) -> Option<DocumentViewValue> {
        self.0.insert(name.to_owned(), value)
    }

    /// Returns the number of added fields.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true when no field is given.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns a field value.
    pub fn get(&self, name: &str) -> Option<&DocumentViewValue> {
        if !self.0.contains_key(name) {
            return None;
        }

        self.0.get(name)
    }

    /// Returns an array of existing document view keys.
    pub fn keys(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    /// Returns an iterator of existing document view fields.
    pub fn iter(&self) -> Iter<String, DocumentViewValue> {
        self.0.iter()
    }
}
