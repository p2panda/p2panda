// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use wasm_bindgen_test::*;

use crate::entry::{Entry, LogId, SeqNum};
use crate::hash::Hash;
use crate::operation::{OperationEncoded, OperationFields, OperationValue};
use crate::test_utils::utils::create_operation;
use crate::wasm::{decode_entry, sign_encode_entry, KeyPair, SignEncodeEntryResult};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn encodes_decodes_entries() {
    let key_pair = KeyPair::new();
    let schema = Hash::new_from_bytes(vec![0, 1, 2]).unwrap();
    let mut fields = OperationFields::new();
    fields
        .add("name", OperationValue::Text("Hello!".to_string()))
        .unwrap();

    let operation = create_operation(schema, fields);
    let operation_encoded = OperationEncoded::try_from(&operation).unwrap();

    // A well formed entries should encode....
    let encode_result = sign_encode_entry(
        &key_pair,
        operation_encoded.as_str().into(),
        None,
        None,
        SeqNum::default().as_i64() as i32,
        LogId::default().as_i64() as i32,
    );

    assert!(encode_result.is_ok());

    let encoded_entry_result: SignEncodeEntryResult = encode_result.unwrap().into_serde().unwrap();

    // ...and decode.
    let decode_result = decode_entry(
        encoded_entry_result.entry_encoded,
        Some(operation_encoded.as_str().into()),
    );

    assert!(decode_result.is_ok());

    let decoded_entry: Entry = decode_result.unwrap().into_serde().unwrap();

    assert_eq!(*decoded_entry.log_id(), LogId::default());
    assert_eq!(*decoded_entry.seq_num(), SeqNum::default());
    assert_eq!(decoded_entry.backlink_hash(), None);
    assert_eq!(decoded_entry.skiplink_hash(), None);
    assert_eq!(decoded_entry.operation(), Some(&operation));

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

    // This entry should have a backlink.
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
