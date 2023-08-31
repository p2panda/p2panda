// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::{
    document::{DocumentId, DocumentViewId},
    operation::error::PlainValueError,
};

/// Operation field values which have not been checked against a schema yet.
///
/// This enum expresses some operation field types as groups, since "String" or "Relation" are
/// represented by the same internal data type (a simple string).
///
/// Latest when combining the plain values with a schema, the inner types, especially the
/// relations, get checked against their correct format.
#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum PlainValue {
    /// Boolean value.
    Boolean(bool),

    /// Integer value.
    Integer(i64),

    /// Float value.
    Float(f64),

    /// Byte string.
    ByteString(ByteBuf),

    /// List of strings which can either be a pinned relation (list of operation ids) a relation
    /// list (list of document ids) or an empty pinned relation list.
    AmbiguousRelation(Vec<String>),

    /// List of a list of strings which is a pinned relation list.
    PinnedRelationList(Vec<Vec<String>>),
}

impl PlainValue {
    /// Returns the string representation of these plain values.
    ///
    /// This is useful for composing error messages or debug logs.
    pub fn field_type(&self) -> &str {
        match self {
            PlainValue::Boolean(_) => "bool",
            PlainValue::ByteString(_) => "byte_string",
            PlainValue::Integer(_) => "int",
            PlainValue::Float(_) => "float",
            PlainValue::AmbiguousRelation(_) => "str[]",
            PlainValue::PinnedRelationList(_) => "str[][]",
        }
    }

    pub fn try_into_string_from_utf8(&self) -> Result<String, PlainValueError> {
        let result = match &self {
            PlainValue::ByteString(bytes) => String::from_utf8(bytes.to_vec()).ok(),
            _ => None,
        };

        match result {
            Some(str_value) => Ok(str_value),
            None => Err(PlainValueError::BytesNotUtf8),
        }
    }
}

impl From<bool> for PlainValue {
    fn from(value: bool) -> Self {
        PlainValue::Boolean(value)
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
        PlainValue::ByteString(ByteBuf::from(value.as_bytes()))
    }
}

impl From<&str> for PlainValue {
    fn from(value: &str) -> Self {
        PlainValue::ByteString(ByteBuf::from(value.as_bytes()))
    }
}

impl From<DocumentId> for PlainValue {
    fn from(value: DocumentId) -> Self {
        PlainValue::ByteString(ByteBuf::from(value.to_string()))
    }
}

impl From<Vec<DocumentId>> for PlainValue {
    fn from(value: Vec<DocumentId>) -> Self {
        PlainValue::AmbiguousRelation(
            value
                .iter()
                .map(|document_id| document_id.to_string())
                .collect(),
        )
    }
}

impl From<DocumentViewId> for PlainValue {
    fn from(value: DocumentViewId) -> Self {
        PlainValue::AmbiguousRelation(
            value
                .iter()
                .map(|operation_id| operation_id.to_string())
                .collect(),
        )
    }
}

impl From<Vec<DocumentViewId>> for PlainValue {
    fn from(value: Vec<DocumentViewId>) -> Self {
        PlainValue::PinnedRelationList(
            value
                .iter()
                .map(|document_view_id| {
                    document_view_id
                        .iter()
                        .map(|operation_id| operation_id.to_string())
                        .collect()
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use rstest::rstest;
    use serde_bytes::ByteBuf;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::serde::{deserialize_into, serialize_from, serialize_value};
    use crate::test_utils::fixtures::{document_id, document_view_id};

    use super::PlainValue;

    #[test]
    fn field_type_representation() {
        assert_eq!("int", PlainValue::Integer(5).field_type());
        assert_eq!("bool", PlainValue::Boolean(false).field_type());
        assert_eq!(
            "byte_string",
            PlainValue::ByteString(ByteBuf::from("test")).field_type()
        );
        assert_eq!(
            "str[]",
            PlainValue::AmbiguousRelation(vec!["test".to_string()]).field_type()
        );
    }

    #[rstest]
    fn from_primitives(document_id: DocumentId, document_view_id: DocumentViewId) {
        // Scalar types
        assert_eq!(PlainValue::Boolean(true), true.into());
        assert_eq!(PlainValue::Float(1.5), 1.5.into());
        assert_eq!(PlainValue::Integer(3), 3.into());
        assert_eq!(
            PlainValue::ByteString(ByteBuf::from("hellö")),
            "hellö".into()
        );

        // Relation types
        assert_eq!(
            PlainValue::ByteString(ByteBuf::from(document_id.to_string())),
            document_id.clone().into()
        );
        assert_eq!(
            PlainValue::AmbiguousRelation(vec![document_id.to_string()]),
            vec![document_id].into()
        );
        assert_eq!(
            PlainValue::AmbiguousRelation(vec![document_view_id.to_string()]),
            document_view_id.clone().into()
        );
        assert_eq!(
            PlainValue::PinnedRelationList(vec![vec![document_view_id.to_string()]]),
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
            serialize_from(PlainValue::AmbiguousRelation(vec![
                "002089e5c6f0cbc0e8d8c92050dffc60e3217b556d62eace0d2e5d374c70a1d0c2d4".to_string()
            ])),
            serialize_value(cbor!([
                "002089e5c6f0cbc0e8d8c92050dffc60e3217b556d62eace0d2e5d374c70a1d0c2d4"
            ]))
        );

        assert_eq!(
            serialize_from(PlainValue::PinnedRelationList(vec![vec![
                "002089e5c6f0cbc0e8d8c92050dffc60e3217b556d62eace0d2e5d374c70a1d0c2d4".to_string()
            ]])),
            serialize_value(cbor!([[
                "002089e5c6f0cbc0e8d8c92050dffc60e3217b556d62eace0d2e5d374c70a1d0c2d4"
            ]]))
        );

        assert_eq!(
            serialize_from(PlainValue::ByteString(ByteBuf::from(vec![0, 1, 2, 3]))),
            serialize_value(cbor!(ByteBuf::from(vec![0, 1, 2, 3])))
        );

        assert_eq!(
            serialize_from(PlainValue::ByteString(ByteBuf::from("Piep"))),
            serialize_value(cbor!(ByteBuf::from("Piep")))
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
            PlainValue::ByteString(ByteBuf::from(vec![0, 1, 2, 3]))
        );
        assert_eq!(
            deserialize_into::<PlainValue>(&serialize_value(cbor!("Piep"))).unwrap(),
            PlainValue::ByteString(ByteBuf::from("Piep"))
        );
        assert_eq!(
            deserialize_into::<PlainValue>(&serialize_value(cbor!([]))).unwrap(),
            PlainValue::AmbiguousRelation(vec![])
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

        // It deserializes a too large integer into a float and passes which is not the expected
        // behaviour, latest when checking against a schema it should fail though!
        let bytes = serialize_value(cbor!(u64::MAX));
        let value = deserialize_into::<PlainValue>(&bytes);
        assert!(value.is_ok());
        assert_eq!(value.unwrap().field_type(), "float");
    }
}
