// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;

use ciborium::Value;
use serde::{Deserialize, Serialize};

use crate::document::{DocumentId, DocumentViewId};
use crate::hash::{Hash, HashId};
use crate::operation::error::PlainValueError;

/// Operation field values which have not been checked against a schema yet.
///
/// This enum expresses some operation field types as groups, since "String" or "Relation" are
/// represented by the same internal data type (a simple string).
///
/// Latest when combining the plain values with a schema, the inner types, especially the
/// relations, get checked against their correct format.
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

    /// List of hashes which can either be a pinned relation (list of operation ids), a relation
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
        let is_human_readable = deserializer.is_human_readable();
        let cbor_value: Value = Deserialize::deserialize(deserializer)?;

        to_plain_value(is_human_readable, cbor_value).map_err(|err| {
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

/// Helper for converting an encoded value into a plain operation value.
fn to_plain_value(is_human_readable: bool, value: Value) -> Result<PlainValue, PlainValueError> {
    let result: Result<PlainValue, PlainValueError> = match value {
        Value::Integer(int) => {
            let int: i64 = int.try_into()?;
            Ok(int.into())
        }
        Value::Bytes(bytes) => Ok(bytes.into()),
        Value::Float(float) => Ok(float.into()),
        Value::Text(text) => Ok(text.into()),
        Value::Bool(bool) => Ok(bool.into()),
        Value::Array(array) => to_plain_value_list(is_human_readable, array),
        _ => return Err(PlainValueError::UnsupportedValue),
    };

    result
}

/// Helper for converting an encoded array into a plain operation list value.
///
/// This method can fail which means the passed value is not an `AmbiguousRelation` or
/// `PinnedRelation` plain value variant.
fn to_plain_value_list(
    is_human_readable: bool,
    array: Vec<Value>,
) -> Result<PlainValue, PlainValueError> {
    // Helper method to convert the given value to a hexadecimal string.
    //
    // If we're working with a human-readable encoding format we can expect the value to already be
    // a hexadecimal string, for non human-readable formats we need to encode it from the bytes
    // first.
    let to_hex_str = |value: &Value| -> Result<String, PlainValueError> {
        if is_human_readable {
            match value.as_text() {
                Some(text) => Ok(text.to_owned()),
                None => Err(PlainValueError::UnsupportedValue),
            }
        } else {
            match value.as_bytes() {
                Some(bytes) => Ok(hex::encode(bytes)),
                None => Err(PlainValueError::UnsupportedValue),
            }
        }
    };

    // First attempt to parse this list of encoded values into a list of hashes. If this succeeds
    // it means this is an `AmbiguousRelation`
    let ambiguous_relation: Result<Vec<Hash>, PlainValueError> = array
        .iter()
        .map(|value| {
            let hex_str = to_hex_str(value)?;
            let hash = Hash::new(&hex_str).map_err(|_| PlainValueError::UnsupportedValue)?;
            Ok(hash)
        })
        .collect();

    // If this was successful we stop here and return the value.
    if let Ok(hashes) = ambiguous_relation {
        return Ok(PlainValue::AmbiguousRelation(hashes));
    };

    // Next we try and parse into a list of `Vec<Hash>` which means this is a `PinnedRelationList`
    // value
    let mut pinned_relations = Vec::new();
    for inner_array in array {
        let inner_array = match inner_array.as_array() {
            Some(array) => Ok(array),
            None => Err(PlainValueError::UnsupportedValue),
        }?;

        let pinned_relation: Result<Vec<Hash>, PlainValueError> = inner_array
            .iter()
            .map(|value| {
                let hex_str = to_hex_str(value)?;
                let hash = Hash::new(&hex_str).map_err(|_| PlainValueError::UnsupportedValue)?;
                Ok(hash)
            })
            .collect();

        pinned_relations.push(pinned_relation?);
    }

    Ok(PlainValue::PinnedRelationList(pinned_relations))
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use rstest::rstest;
    use serde_bytes::ByteBuf;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::hash::{Hash, HashId};
    use crate::serde::{deserialize_into, hex_string_to_bytes, serialize_from, serialize_value};
    use crate::test_utils::fixtures::{document_id, document_view_id, random_hash};

    use super::PlainValue;

    #[test]
    fn field_type_representation() {
        assert_eq!("int", PlainValue::Integer(5).field_type());
        assert_eq!("bool", PlainValue::Boolean(false).field_type());
        assert_eq!(
            "bytes",
            PlainValue::BytesOrRelation("test".as_bytes().into()).field_type()
        );
        assert_eq!("str", PlainValue::String("test".into()).field_type());
        assert_eq!(
            "hash[]",
            PlainValue::AmbiguousRelation(vec![random_hash()]).field_type()
        );
    }

    #[rstest]
    fn from_primitives(document_id: DocumentId, document_view_id: DocumentViewId) {
        // Scalar types
        assert_eq!(PlainValue::Boolean(true), true.into());
        assert_eq!(PlainValue::Float(1.5), 1.5.into());
        assert_eq!(PlainValue::Integer(3), 3.into());
        assert_eq!(
            PlainValue::BytesOrRelation("hellö".as_bytes().to_vec()),
            "hellö".as_bytes().into()
        );
        assert_eq!(PlainValue::String("hellö".to_string()), "hellö".into());

        // Relation types
        assert_eq!(
            PlainValue::BytesOrRelation(document_id.to_bytes()),
            document_id.clone().into()
        );
        assert_eq!(
            PlainValue::AmbiguousRelation(vec![document_id.clone().into()]),
            vec![document_id].into()
        );
        assert_eq!(
            PlainValue::AmbiguousRelation(document_view_id.clone().into()),
            document_view_id.clone().into()
        );
        assert_eq!(
            PlainValue::PinnedRelationList(vec![document_view_id.clone().into()]),
            vec![document_view_id].into()
        );
    }

    #[test]
    fn serialize() {
        assert_eq!(
            serialize_from(PlainValue::Integer(5)),
            serialize_value(cbor!(5))
        );

        assert_eq!(
            serialize_from(PlainValue::AmbiguousRelation(vec![Hash::new(
                "002089e5c6f0cbc0e8d8c92050dffc60e3217b556d62eace0d2e5d374c70a1d0c2d4"
            )
            .unwrap()])),
            serialize_value(cbor!([hex_string_to_bytes(
                "002089e5c6f0cbc0e8d8c92050dffc60e3217b556d62eace0d2e5d374c70a1d0c2d4"
            )]))
        );

        assert_eq!(
            serialize_from(PlainValue::PinnedRelationList(vec![vec![Hash::new(
                "002089e5c6f0cbc0e8d8c92050dffc60e3217b556d62eace0d2e5d374c70a1d0c2d4"
            )
            .unwrap()]])),
            serialize_value(cbor!([[hex_string_to_bytes(
                "002089e5c6f0cbc0e8d8c92050dffc60e3217b556d62eace0d2e5d374c70a1d0c2d4"
            )]]))
        );

        assert_eq!(
            serialize_from(PlainValue::BytesOrRelation(vec![0, 1, 2, 3])),
            serialize_value(cbor!(ByteBuf::from(vec![0, 1, 2, 3])))
        );

        assert_eq!(
            serialize_from(PlainValue::String("username".to_string())),
            serialize_value(cbor!("username"))
        );

        assert_eq!(
            serialize_from(PlainValue::AmbiguousRelation(vec![])),
            serialize_value(cbor!([]))
        );
    }

    #[test]
    fn deserialize() {
        assert_eq!(
            deserialize_into::<PlainValue>(&serialize_value(cbor!(12))).unwrap(),
            PlainValue::Integer(12)
        );
        assert_eq!(
            deserialize_into::<PlainValue>(&serialize_value(cbor!(12.0))).unwrap(),
            PlainValue::Float(12.0)
        );
        assert_eq!(
            deserialize_into::<PlainValue>(&serialize_value(cbor!(ByteBuf::from(vec![
                0, 1, 2, 3
            ]))))
            .unwrap(),
            PlainValue::BytesOrRelation(vec![0, 1, 2, 3])
        );
        assert_eq!(
            deserialize_into::<PlainValue>(&serialize_value(cbor!("hello"))).unwrap(),
            PlainValue::String("hello".to_string())
        );
        assert_eq!(
            deserialize_into::<PlainValue>(&serialize_value(cbor!([]))).unwrap(),
            PlainValue::AmbiguousRelation(vec![])
        );
    }

    #[test]
    fn deserialize_human_readable() {
        assert_eq!(
            serde_json::from_str::<PlainValue>("12").unwrap(),
            PlainValue::Integer(12)
        );
        assert_eq!(
            serde_json::from_str::<PlainValue>("12.0").unwrap(),
            PlainValue::Float(12.0)
        );
        assert_eq!(
            serde_json::from_str::<PlainValue>("\"hello\"").unwrap(),
            PlainValue::String("hello".to_string())
        );
        assert_eq!(
            serde_json::from_str::<PlainValue>("[]").unwrap(),
            PlainValue::AmbiguousRelation(vec![])
        );
        assert_eq!(
            serde_json::from_str::<PlainValue>(
                "[[\"00200801063d8aaba76c283a2fb63cf5cd4ec86765424452ce7327fda04c5da80d62\"]]"
            )
            .unwrap(),
            PlainValue::PinnedRelationList(vec![vec![Hash::new(
                "00200801063d8aaba76c283a2fb63cf5cd4ec86765424452ce7327fda04c5da80d62"
            )
            .unwrap()]])
        );
    }

    #[test]
    fn large_numbers() {
        assert_eq!(
            deserialize_into::<PlainValue>(&serialize_value(cbor!(i64::MAX))).unwrap(),
            PlainValue::Integer(i64::MAX)
        );
        assert_eq!(
            deserialize_into::<PlainValue>(&serialize_value(cbor!(f64::MAX))).unwrap(),
            PlainValue::Float(f64::MAX)
        );

        // It errors when deserializing a too large number
        let bytes = serialize_value(cbor!(u64::MAX));
        let value = deserialize_into::<PlainValue>(&bytes);
        assert!(value.is_err());
    }
}
