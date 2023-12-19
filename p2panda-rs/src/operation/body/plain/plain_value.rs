// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;

use ciborium::Value;
use serde::{Deserialize, Serialize};

use crate::document::{DocumentId, DocumentViewId};
use crate::hash::{Hash, HashId};
use crate::operation::body::error::PlainValueError;

#[derive(Serialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum PlainValue {
    /// Boolean value.
    Boolean(bool),

    /// Integer value.
    Integer(i64),

    /// Float value.
    Float(f64),

    /// String value.
    String(String),

    /// Byte array value which can either represent raw bytes or a relation (document id)
    /// encoded as bytes.
    #[serde(with = "serde_bytes")]
    BytesOrRelation(Vec<u8>),

    /// List of hashes which can either be a pinned relation (list of operation ids) a relation
    /// list (list of document ids) or an empty pinned relation list.
    AmbiguousRelation(Vec<Hash>),

    /// List of a list of hashes which is a pinned relation list.
    PinnedRelationList(Vec<Vec<Hash>>),
}

impl PlainValue {
    /// Returns the string representation of these plain values.
    ///
    /// This is useful for composing error messages or debug logs.
    pub fn field_type(&self) -> &str {
        match self {
            PlainValue::Boolean(_) => "bool",
            PlainValue::Integer(_) => "int",
            PlainValue::Float(_) => "float",
            PlainValue::String(_) => "str",
            PlainValue::BytesOrRelation(_) => "bytes",
            PlainValue::AmbiguousRelation(_) => "hash[]",
            PlainValue::PinnedRelationList(_) => "hash[][]",
        }
    }
}

impl<'de> Deserialize<'de> for PlainValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let cbor_value: Value = Deserialize::deserialize(deserializer)?;

        cbor_value_to_plain_value(cbor_value).map_err(|err| {
            serde::de::Error::custom(format!("error deserializing plain value: {}", err))
        })
    }
}

impl From<bool> for PlainValue {
    fn from(value: bool) -> Self {
        PlainValue::Boolean(value)
    }
}

impl From<Vec<u8>> for PlainValue {
    fn from(value: Vec<u8>) -> Self {
        PlainValue::BytesOrRelation(value)
    }
}

impl From<&[u8]> for PlainValue {
    fn from(value: &[u8]) -> Self {
        PlainValue::BytesOrRelation(value.to_owned())
    }
}

impl From<f64> for PlainValue {
    fn from(value: f64) -> Self {
        PlainValue::Float(value)
    }
}

impl From<i64> for PlainValue {
    fn from(value: i64) -> Self {
        PlainValue::Integer(value)
    }
}

impl From<String> for PlainValue {
    fn from(value: String) -> Self {
        PlainValue::String(value)
    }
}

impl From<Vec<Hash>> for PlainValue {
    fn from(value: Vec<Hash>) -> Self {
        PlainValue::AmbiguousRelation(value)
    }
}

impl From<&str> for PlainValue {
    fn from(value: &str) -> Self {
        PlainValue::String(value.to_owned())
    }
}

impl From<DocumentId> for PlainValue {
    fn from(value: DocumentId) -> Self {
        PlainValue::BytesOrRelation(hex::decode(value.as_str()).unwrap())
    }
}

impl From<Vec<DocumentId>> for PlainValue {
    fn from(value: Vec<DocumentId>) -> Self {
        PlainValue::AmbiguousRelation(value.iter().map(HashId::as_hash).cloned().collect())
    }
}

impl From<DocumentViewId> for PlainValue {
    fn from(value: DocumentViewId) -> Self {
        PlainValue::AmbiguousRelation(value.into())
    }
}

impl From<Vec<DocumentViewId>> for PlainValue {
    fn from(value: Vec<DocumentViewId>) -> Self {
        PlainValue::PinnedRelationList(value.iter().cloned().map(Into::<Vec<Hash>>::into).collect())
    }
}

/// Helper for converting a cbor value into a plain operation value.
fn cbor_value_to_plain_value(value: Value) -> Result<PlainValue, PlainValueError> {
    let result: Result<PlainValue, PlainValueError> = match value {
        Value::Integer(int) => {
            let int: i64 = int.try_into()?;
            Ok(int.into())
        }
        Value::Bytes(bytes) => Ok(bytes.into()),
        Value::Float(float) => Ok(float.into()),
        Value::Text(text) => Ok(text.into()),
        Value::Bool(bool) => Ok(bool.into()),
        Value::Array(array) => cbor_array_to_plain_value_list(array),
        _ => return Err(PlainValueError::UnsupportedValue),
    };

    result
}

/// Helper for converting a cbor array into a plain operation list value.
///
/// This method can fail which means the passed value is not an `AmbiguousRelation` or
/// `PinnedRelation` plain value variant.
fn cbor_array_to_plain_value_list(array: Vec<Value>) -> Result<PlainValue, PlainValueError> {
    // First attempt to parse this vec of values into a vec of strings. If this succeeds it means
    // this is an `AmbiguousRelation`
    let ambiguous_relation: Result<Vec<Hash>, _> = array
        .iter()
        .map(|value| match value.as_bytes() {
            Some(bytes) => {
                let hex_str = hex::encode(bytes);
                let hash = Hash::new(&hex_str).map_err(|_| PlainValueError::UnsupportedValue)?;
                Ok(hash)
            }
            None => Err(PlainValueError::UnsupportedValue),
        })
        .collect();

    // If this was successful we stop here and return the value.
    if let Ok(hashes) = ambiguous_relation {
        return Ok(PlainValue::AmbiguousRelation(hashes));
    };

    // Next we try and parse into a vec of `Vec<String>` which means this is a
    // `PinnedRelationList` value
    let mut pinned_relations = Vec::new();
    for inner_array in array {
        let inner_array = match inner_array.as_array() {
            Some(array) => Ok(array),
            None => Err(PlainValueError::UnsupportedValue),
        }?;
        let pinned_relation: Result<Vec<Hash>, _> = inner_array
            .iter()
            .map(|value| match value.as_bytes() {
                Some(bytes) => {
                    let hex_str = hex::encode(bytes);
                    let hash =
                        Hash::new(&hex_str).map_err(|_| PlainValueError::UnsupportedValue)?;
                    Ok(hash)
                }
                None => Err(PlainValueError::UnsupportedValue),
            })
            .collect();

        pinned_relations.push(pinned_relation?);
    }

    Ok(PlainValue::PinnedRelationList(pinned_relations))
}
