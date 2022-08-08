// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::next::entry::decode::decode_entry as decode;
use crate::next::entry::encode::sign_and_encode_entry;
use crate::next::entry::{EncodedEntry, Entry, LogId, SeqNum};
use crate::next::hash::Hash;
use crate::next::operation::decode::decode_operation;
use crate::next::operation::plain::PlainOperation;
use crate::next::operation::EncodedOperation;
use crate::next::wasm::error::jserr;
use crate::next::wasm::serde::serialize_to_js;
use crate::next::wasm::KeyPair;

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

/// Return value of [`decode_entry`] that holds the decoded entry and plain operation.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DecodeEntryResult {
    /// Author of this entry.
    pub public_key: String,

    /// Used log for this entry.
    pub log_id: u64,

    /// Sequence number of this entry.
    pub seq_num: u64,

    /// Hash of skiplink Bamboo entry.
    pub skiplink: Option<String>,

    /// Hash of previous Bamboo entry.
    pub backlink: Option<String>,

    /// Ed25519 signature of entry.
    pub signature: String,

    /// Payload size of entry.
    pub payload_size: u64,

    /// Hash of payload.
    pub payload_hash: String,

    /// Plain operation object, payload of this entry.
    pub operation: Option<PlainOperation>,
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
    // If skiplink_hash exists construct `Hash`
    let skiplink_hash = match entry_skiplink_hash {
        Some(hash) => Some(jserr!(Hash::new(&hash))),
        None => None,
    };

    // If backlink_hash exists construct `Hash`
    let backlink_hash = match entry_backlink_hash {
        Some(hash) => Some(jserr!(Hash::new(&hash))),
        None => None,
    };

    // Create `SeqNum` instance
    let seq_num = jserr!(SeqNum::new(seq_num));

    // Convert to `EncodedOperation`
    let operation_bytes = jserr!(
        hex::decode(encoded_operation),
        "Invalid hex-encoding of encoded operation"
    );
    let operation_encoded = EncodedOperation::from_bytes(&operation_bytes);

    // Sign and encode entry
    let entry_encoded = jserr!(sign_and_encode_entry(
        &LogId::new(log_id),
        &seq_num,
        skiplink_hash.as_ref(),
        backlink_hash.as_ref(),
        &operation_encoded,
        key_pair.as_inner(),
    ));

    // Serialise result to JavaScript object
    let entry_operation_bundle = SignEncodeEntryResult {
        entry_encoded: entry_encoded.to_string(),
        entry_hash: entry_encoded.hash().to_string(),
        operation_hash: operation_encoded.hash().to_string(),
    };
    let result = jserr!(serialize_to_js(&entry_operation_bundle));
    Ok(result)
}

/// Decodes an entry and optional operation given their encoded form.
#[wasm_bindgen(js_name = decodeEntry)]
pub fn decode_entry(entry_str: String, operation_str: Option<String>) -> Result<JsValue, JsValue> {
    // Convert encoded operation
    let operation_plain = match operation_str {
        Some(hex_str) => {
            let operation_bytes = jserr!(
                hex::decode(hex_str),
                "Invalid hex-encoding of encoded operation"
            );
            let operation_encoded = EncodedOperation::from_bytes(&operation_bytes);

            // Decode to plain operation
            // @TODO: We want actual operations here, but for this we need schemas
            let operation_plain = jserr!(decode_operation(&operation_encoded));

            Some(operation_plain)
        }
        None => None,
    };

    // Convert encoded entry
    let entry_bytes = jserr!(
        hex::decode(entry_str),
        "Invalid hex-encoding of encoded entry"
    );
    let entry_encoded = EncodedEntry::from_bytes(&entry_bytes);

    // Decode entry
    let entry: Entry = jserr!(decode(&entry_encoded));

    // Serialise result to JavaScript object
    let entry_operation_bundle = DecodeEntryResult {
        public_key: entry.public_key().to_string(),
        seq_num: entry.seq_num().as_u64(),
        log_id: entry.log_id().as_u64(),
        skiplink: entry.skiplink().map(|hash| hash.to_string()),
        backlink: entry.backlink().map(|hash| hash.to_string()),
        payload_size: entry.payload_size(),
        payload_hash: entry.payload_hash().to_string(),
        signature: entry.signature().to_string(),
        // @TODO: Return full operation here, not only schema-less plain operation
        operation: operation_plain,
    };
    let result = jserr!(serialize_to_js(&entry_operation_bundle));
    Ok(result)
}
