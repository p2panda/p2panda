// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

type RawVersion = u64;

type RawAction = u64;

type RawPreviousOperations = Vec<String>;

type RawSchemaId = String;

type RawFields = BTreeMap<String, RawValue>;

#[derive(Deserialize, Serialize, Debug, PartialEq)]
enum RawValue {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    Text(String),
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct RawOperation(
    RawVersion,
    RawAction,
    Option<RawPreviousOperations>,
    RawSchemaId,
    Option<RawFields>,
);

impl RawOperation {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&self, &mut cbor_bytes).unwrap();
        cbor_bytes
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{RawFields, RawOperation, RawValue};

    #[test]
    fn encode_decode() {
        let mut raw_fields = BTreeMap::new();
        raw_fields.insert("name".to_owned(), RawValue::Text("venue".to_owned()));
        raw_fields.insert("type".to_owned(), RawValue::Text("str".to_owned()));

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
                133, 0, 0, 246, 120, 26, 115, 99, 104, 101, 109, 97, 95, 102, 105, 101, 108, 100,
                95, 100, 101, 102, 105, 110, 105, 116, 105, 111, 110, 95, 118, 49, 162, 100, 110,
                97, 109, 101, 161, 100, 84, 101, 120, 116, 101, 118, 101, 110, 117, 101, 100, 116,
                121, 112, 101, 161, 100, 84, 101, 120, 116, 99, 115, 116, 114
            ],
        );

        let decoded_operation: RawOperation = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(decoded_operation, raw_operation);
    }
}
