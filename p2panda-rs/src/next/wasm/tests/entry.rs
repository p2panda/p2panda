// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use wasm_bindgen_test::*;

use crate::next::document::DocumentViewId;
use crate::next::hash::Hash;
use crate::next::operation::{EncodedOperation, OperationFields, OperationValue};
use crate::next::schema::SchemaId;
use crate::next::test_utils::fixtures::operation;
use crate::next::wasm::{decode_entry, sign_encode_entry, KeyPair, SignEncodeEntryResult};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn encodes_decodes_entries() {
    let key_pair = KeyPair::new();
    let schema = SchemaId::Application(
        "profile".to_string(),
        "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
            .parse::<DocumentViewId>()
            .unwrap(),
    );

    let mut fields = OperationFields::new();
    fields
        .add("name", OperationValue::Text("Hello!".to_string()))
        .unwrap();

    let operation = operation(Some(fields), None, Some(schema));
    let operation_encoded = OperationEncoded::try_from(&operation).unwrap();

    // Encode correct entry
    let encode_result = sign_encode_entry(
        &key_pair,
        operation_encoded.as_str().into(),
        None,
        None,
        1,
        1,
    );
    assert!(encode_result.is_ok());

    let encoded_entry_result: SignEncodeEntryResult =
        serde_wasm_bindgen::from_value(encode_result.unwrap()).unwrap();

    // ... and decode again
    let decode_result = decode_entry(
        encoded_entry_result.entry_encoded,
        Some(operation_encoded.as_str().into()),
    );
    assert!(decode_result.is_ok());

    // @TODO: This currently does not work because of an issue in the `serde_wasm_bindgen` crate
    // not allowing us to convert JsValue(BigInt) values back into Rust types.
    //
    // Related issue: https://github.com/cloudflare/serde-wasm-bindgen/issues/30
    //
    // let decoded_entry: Entry = deserialize_from_js(decode_result.unwrap()).unwrap();
    // assert_eq!(*decoded_entry.log_id(), LogId::new(1));
    // assert_eq!(*decoded_entry.seq_num(), SeqNum::new(1).unwrap());
    // assert_eq!(decoded_entry.backlink_hash(), None);
    // assert_eq!(decoded_entry.skiplink_hash(), None);
    // assert_eq!(decoded_entry.operation(), Some(&operation)); */
    // Entries with backlink and skiplink should encode
    let result = sign_encode_entry(
        &key_pair,
        operation_encoded.as_str().into(),
        Some(Hash::new_from_bytes(&[0, 1, 2]).to_string()),
        Some(Hash::new_from_bytes(&[1, 2, 3]).to_string()),
        7,
        1,
    );
    assert!(result.is_ok());

    // This entry should have a backlink and is invalid
    let result = sign_encode_entry(&key_pair, operation_encoded.to_string(), None, None, 3, 1);
    assert!(result.is_err());
}
