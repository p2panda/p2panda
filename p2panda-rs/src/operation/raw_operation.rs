// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

type RawVersion = u64;

type RawAction = u64;

type RawPreviousOperations = Option<Vec<String>>;

type RawSchemaId = String;

type RawFields = Option<BTreeMap<String, RawValue>>;

#[derive(Deserialize, Serialize, Debug)]
enum RawValue {
    Boolean(bool),
    Float(f64),
    Integer(i64),
    String(String),
}

#[derive(Deserialize, Serialize, Debug)]
pub struct RawOperation(
    RawVersion,
    RawAction,
    RawPreviousOperations,
    RawSchemaId,
    RawFields,
);

impl RawOperation {
    pub fn to_bytes(&self) -> Vec<u8> {
        valuable_value::compact::to_vec(self).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::Deserialize;

    use super::{RawFields, RawOperation, RawValue};

    #[test]
    fn serialize() {
        let mut raw_fields = BTreeMap::new();
        raw_fields.insert("name".to_owned(), RawValue::String("venue".to_owned()));
        raw_fields.insert("type".to_owned(), RawValue::String("str".to_owned()));

        let raw_operation = RawOperation(
            0,
            0,
            None,
            "schema_field_definition_v1".to_owned(),
            Some(raw_fields),
        );
        let bytes = raw_operation.to_bytes();

        assert_eq!(
            bytes,
            vec![
                165, 96, 96, 132, 78, 111, 110, 101, 154, 115, 99, 104, 101, 109, 97, 95, 102, 105,
                101, 108, 100, 95, 100, 101, 102, 105, 110, 105, 116, 105, 111, 110, 95, 118, 49,
                225, 132, 83, 111, 109, 101, 226, 132, 110, 97, 109, 101, 225, 134, 83, 116, 114,
                105, 110, 103, 133, 118, 101, 110, 117, 101, 132, 116, 121, 112, 101, 225, 134, 83,
                116, 114, 105, 110, 103, 131, 115, 116, 114
            ]
        );
    }

    #[test]
    fn deserialize() {
        let bytes: Vec<u8> = vec![
            165, 96, 96, 132, 78, 111, 110, 101, 154, 115, 99, 104, 101, 109, 97, 95, 102, 105,
            101, 108, 100, 95, 100, 101, 102, 105, 110, 105, 116, 105, 111, 110, 95, 118, 49, 225,
            132, 83, 111, 109, 101, 226, 132, 110, 97, 109, 101, 225, 134, 83, 116, 114, 105, 110,
            103, 133, 118, 101, 110, 117, 101, 132, 116, 121, 112, 101, 225, 134, 83, 116, 114,
            105, 110, 103, 131, 115, 116, 114,
        ];

        RawOperation::deserialize(&bytes);
    }
}
