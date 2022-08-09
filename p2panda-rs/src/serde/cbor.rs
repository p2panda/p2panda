// SPDX-License-Identifier: AGPL-3.0-or-later

use ciborium::ser::into_writer;
use ciborium::value::{Error, Value};

/// Helper method for tests to encode values generated with the `cbor!` macro into bytes.
pub fn encode_value(value: Result<Value, Error>) -> Vec<u8> {
    let mut cbor_bytes = Vec::new();
    into_writer(&value.expect("Invalid CBOR value"), &mut cbor_bytes).unwrap();
    cbor_bytes
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;

    use super::encode_value;

    #[test]
    fn encode() {
        assert_eq!(vec![24, 42], encode_value(cbor!(42)));
    }
}
