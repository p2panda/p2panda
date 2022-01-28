// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::entry::{decode_entry as decode, sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::operation::{Operation, OperationEncoded};
use crate::wasm::error::jserr;
use crate::wasm::serde::serialize_to_js;
use crate::wasm::KeyPair;

/// Return value of [`sign_encode_entry`] that holds the encoded entry and its hash.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SignEncodeEntryResult {
    /// Encoded p2panda entry.
    pub entry_encoded: String,

    /// The hash of a p2panda entry.
    pub entry_hash: String,

    /// The hash of a p2panda operation.
    pub operation_hash: String,
}

/// Returns a signed and encoded entry that can be published to a p2panda node.
///
/// `entry_backlink_hash`, `entry_skiplink_hash`, `seq_num` and `log_id` are obtained by querying
/// the `getEntryArguments` method of a p2panda node.
#[wasm_bindgen(js_name = signEncodeEntry)]
pub fn sign_encode_entry(
    key_pair: &KeyPair,
    encoded_operation: String,
    entry_skiplink_hash: Option<String>,
    entry_backlink_hash: Option<String>,
    seq_num: u64,
    log_id: u64,
) -> Result<JsValue, JsValue> {
    // If skiplink_hash exists construct Hash
    let skiplink_hash = match entry_skiplink_hash {
        Some(hash) => Some(jserr!(Hash::new(&hash))),
        None => None,
    };

    // If backlink_hash exists construct Hash
    let backlink_hash = match entry_backlink_hash {
        Some(hash) => Some(jserr!(Hash::new(&hash))),
        None => None,
    };

    // Create SeqNum instance
    let seq_num = jserr!(SeqNum::new(seq_num));

    // Convert to Operation
    let operation_encoded = jserr!(OperationEncoded::new(&encoded_operation));
    let operation = jserr!(Operation::try_from(&operation_encoded));

    // Create Entry instance
    let entry = jserr!(Entry::new(
        &LogId::new(log_id),
        Some(&operation),
        skiplink_hash.as_ref(),
        backlink_hash.as_ref(),
        &seq_num,
    ));

    // Finally sign and encode entry
    let entry_signed = jserr!(sign_and_encode(&entry, key_pair.as_inner()));

    // Serialise result to JavaScript object
    let entry_operation_bundle = SignEncodeEntryResult {
        entry_encoded: entry_signed.as_str().into(),
        entry_hash: entry_signed.hash().as_str().into(),
        operation_hash: operation_encoded.hash().as_str().into(),
    };
    let result = jserr!(serialize_to_js(&entry_operation_bundle));
    Ok(result)
}

/// Decodes an entry and optional operation given their encoded form.
#[wasm_bindgen(js_name = decodeEntry)]
pub fn decode_entry(
    entry_encoded: String,
    operation_encoded: Option<String>,
) -> Result<JsValue, JsValue> {
    // Convert encoded operation
    let operation_encoded = match operation_encoded {
        Some(msg) => {
            let inner = jserr!(OperationEncoded::new(&msg));
            Some(inner)
        }
        None => None,
    };

    // Convert encoded entry
    let entry_signed = jserr!(EntrySigned::new(&entry_encoded));
    let entry: Entry = jserr!(decode(&entry_signed, operation_encoded.as_ref()));

    // Serialize struct to JavaScript object.
    let result = jserr!(serialize_to_js(&entry));
    Ok(result)
}
