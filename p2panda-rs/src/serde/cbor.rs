// SPDX-License-Identifier: AGPL-3.0-or-later

use ciborium::ser::into_writer;
use ciborium::value::Value;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Helper method for tests to encode values generated with the `cbor!` macro into bytes.
pub fn serialize_value(value: Result<Value, ciborium::value::Error>) -> Vec<u8> {
    let mut cbor_bytes = Vec::new();
    into_writer(&value.expect("Invalid ciborium value"), &mut cbor_bytes).unwrap();
    cbor_bytes
}

/// Helper method for tests to encode CBOR from any serializable struct.
pub fn serialize_from<T>(value: T) -> Vec<u8>
where
    T: Serialize + Sized,
{
    let mut cbor_bytes = Vec::new();
    into_writer(&value, &mut cbor_bytes).unwrap();
    cbor_bytes
}

/// Helper method to deserialize from CBOR bytes into any struct.
pub fn deserialize_into<'de, T>(cbor_bytes: &[u8]) -> Result<T, ciborium::de::Error<std::io::Error>>
where
    T: DeserializeOwned + Sized,
{
    ciborium::de::from_reader(cbor_bytes)
}
//
// #[cfg(test)]
// mod tests {
//     use ciborium::cbor;
//
//     use super::{deserialize_into, serialize_from, serialize_value};
//
//     #[test]
//     fn encode() {
//         assert_eq!(vec![24, 42], serialize_value(cbor!(42)));
//     }
//
//     #[test]
//     fn serialize() {
//         let bytes = serialize_from(LogId::new(12));
//         assert_eq!(bytes, vec![12]);
//     }
//
//     #[test]
//     fn deserialize() {
//         let log_id: LogId = deserialize_into(&serialize_value(cbor!(12))).unwrap();
//         assert_eq!(log_id, LogId::new(12));
//     }
// }
