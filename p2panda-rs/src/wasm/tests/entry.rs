// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use wasm_bindgen_test::*;

use crate::document::DocumentId;
use crate::hash::Hash;
use crate::operation::{OperationEncoded, OperationFields, OperationValue, Relation};
use crate::schema::SchemaId;
use crate::test_utils::utils::create_operation;
use crate::wasm::{decode_entry, sign_encode_entry, KeyPair, SignEncodeEntryResult};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn encodes_decodes_entries() {
    let key_pair = KeyPair::new();
    let schema = SchemaId::Application(Relation::new(DocumentId::new(
        Hash::new_from_bytes(vec![1, 2, 3]).unwrap(),
    )));

    let mut fields = OperationFields::new();
    fields
        .add("name", OperationValue::Text("Hello!".to_string()))
        .unwrap();

    let operation = create_operation(schema, fields);
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
        Some(
            Hash::new_from_bytes(vec![0, 1, 2])
                .unwrap()
                .as_str()
                .to_string(),
        ),
        Some(
            Hash::new_from_bytes(vec![1, 2, 3])
                .unwrap()
                .as_str()
                .to_string(),
        ),
        7,
        1,
    );
    assert!(result.is_ok());

    // This entry should have a backlink and is invalid
    let result = sign_encode_entry(
        &key_pair,
        operation_encoded.as_str().into(),
        None,
        None,
        3,
        1,
    );
    assert!(result.is_err());
}
