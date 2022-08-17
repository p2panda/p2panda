// SPDX-License-Identifier: AGPL-3.0-or-later

use wasm_bindgen_test::*;

use crate::hash::Hash;
use crate::operation::OperationFields;
use crate::test_utils::fixtures::{operation_with_schema, random_document_view_id};
use crate::wasm::{decode_entry, sign_and_encode_entry, KeyPair};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn encodes_decodes_entries() {
    // Prepare key pair
    let key_pair = KeyPair::new();

    // Prepare operation
    let mut fields = OperationFields::new();
    fields.insert("username", "dolphin".into()).unwrap();
    let operation = operation_with_schema(Some(fields), Some(random_document_view_id()));
    let operation_encoded = crate::operation::encode::encode_operation(&operation).unwrap();

    // Encode entry
    let encoded_entry =
        sign_and_encode_entry(0, 1, None, None, operation_encoded.to_string(), &key_pair);
    assert!(encoded_entry.is_ok());

    // ... and decode again
    let decode_result = decode_entry(encoded_entry.unwrap());
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
    // assert_eq!(decoded_entry.operation(), Some(&operation));

    // Entries with backlink and skiplink should encode
    let result = sign_and_encode_entry(
        0,
        7,
        Some(Hash::new_from_bytes(&[0, 1, 2]).to_string()),
        Some(Hash::new_from_bytes(&[1, 2, 3]).to_string()),
        operation_encoded.to_string(),
        &key_pair,
    );
    assert!(result.is_ok());

    // This entry should have a backlink and is invalid
    let result = sign_and_encode_entry(0, 3, None, None, operation_encoded.to_string(), &key_pair);
    assert!(result.is_err());
}
