// SPDX-License-Identifier: AGPL-3.0-or-later

use wasm_bindgen_test::*;

use crate::hash::Hash;
use crate::operation::encode::encode_operation;
use crate::operation::OperationFields;
use crate::test_utils::fixtures::{operation_with_schema, random_document_view_id};
use crate::wasm::{decode_entry, sign_encode_entry, KeyPair, SignEncodeEntryResult};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn encodes_decodes_entries() {
    let key_pair = KeyPair::new();

    let mut fields = OperationFields::new();
    fields.insert("username", "dolphin".into()).unwrap();

    let operation = operation_with_schema(Some(fields), Some(random_document_view_id()));
    let operation_encoded = encode_operation(&operation).unwrap();

    // Encode correct entry
    let encode_result =
        sign_encode_entry(&key_pair, operation_encoded.to_string(), None, None, 1, 1);
    assert!(encode_result.is_ok());

    let encoded_entry_result: SignEncodeEntryResult =
        serde_wasm_bindgen::from_value(encode_result.unwrap()).unwrap();

    // ... and decode again
    let decode_result = decode_entry(
        encoded_entry_result.entry_encoded,
        Some(operation_encoded.to_string()),
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
        operation_encoded.to_string(),
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
