// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(test)]
use serde::Deserialize;
use serde::Serialize;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::entry::traits::AsEntry;
use crate::entry::{EncodedEntry, LogId, SeqNum};
use crate::hash::Hash;
use crate::operation::EncodedOperation;
use crate::wasm::error::jserr;
use crate::wasm::serde::serialize_to_js;
use crate::wasm::KeyPair;

/// Return value of [`decode_entry`] that holds the decoded entry and plain operation.
#[derive(Serialize, Debug)]
#[cfg_attr(test, derive(Deserialize))]
#[serde(rename_all = "camelCase")]
pub struct Entry {
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

    /// Payload size of entry.
    pub payload_size: u64,

    /// Hash of payload.
    pub payload_hash: String,

    /// Ed25519 signature of entry.
    pub signature: String,
}

/// Returns a signed Bamboo entry.
#[wasm_bindgen(js_name = signAndEncodeEntry)]
pub fn sign_and_encode_entry(
    log_id: u64,
    seq_num: u64,
    skiplink_hash: Option<String>,
    backlink_hash: Option<String>,
    payload: String,
    key_pair: &KeyPair,
) -> Result<String, JsValue> {
    // If skiplink_hash exists construct `Hash`
    let skiplink = match skiplink_hash {
        Some(hash) => Some(jserr!(Hash::new(&hash))),
        None => None,
    };

    // If backlink_hash exists construct `Hash`
    let backlink = match backlink_hash {
        Some(hash) => Some(jserr!(Hash::new(&hash))),
        None => None,
    };

    // Convert `SeqNum` and `LogId`
    let log_id = LogId::new(log_id);
    let seq_num = jserr!(SeqNum::new(seq_num));

    // Convert to `EncodedOperation`
    let operation_bytes = jserr!(
        hex::decode(payload),
        "Invalid hex-encoding of encoded operation"
    );
    let operation_encoded = EncodedOperation::from_bytes(&operation_bytes);

    // Sign and encode entry
    let entry_encoded = jserr!(crate::entry::encode::sign_and_encode_entry(
        &log_id,
        &seq_num,
        skiplink.as_ref(),
        backlink.as_ref(),
        &operation_encoded,
        key_pair.as_inner(),
    ));

    // Return result as a hexadecimal string
    Ok(entry_encoded.to_string())
}

/// Decodes an hexadecimal string into an `Entry`.
#[wasm_bindgen(js_name = decodeEntry)]
pub fn decode_entry(encoded_entry: String) -> Result<JsValue, JsValue> {
    // Convert hexadecimal string to bytes
    let entry_bytes = jserr!(
        hex::decode(encoded_entry),
        "Invalid hex-encoding of encoded entry"
    );
    let entry_encoded = EncodedEntry::from_bytes(&entry_bytes);

    // Decode Bamboo entry
    let entry: crate::entry::Entry = jserr!(crate::entry::decode::decode_entry(&entry_encoded));

    // Serialise result to JavaScript object
    let wasm_entry = Entry {
        public_key: entry.public_key().to_string(),
        seq_num: entry.seq_num().as_u64(),
        log_id: entry.log_id().as_u64(),
        skiplink: entry.skiplink().map(|hash| hash.to_string()),
        backlink: entry.backlink().map(|hash| hash.to_string()),
        payload_size: entry.payload_size(),
        payload_hash: entry.payload_hash().to_string(),
        signature: entry.signature().to_string(),
    };
    let result = jserr!(serialize_to_js(&wasm_entry));
    Ok(result)
}
