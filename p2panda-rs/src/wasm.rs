use std::convert::TryFrom;
use std::panic;

use console_error_panic_hook::hook as panic_hook;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::atomic::{
    Entry, EntrySigned, Hash, LogId, Message, MessageEncoded,
    MessageFields as MessageFieldsNonWasm, MessageValue, SeqNum,
};
use crate::key_pair::KeyPair;
use crate::encoder::{sign_and_encode, decode};

// Converts any Rust Error type into js_sys:Error while keeping its error
// message. This helps propagating errors similar like we do in Rust but in
// WebAssembly contexts.
macro_rules! jserr {
    ($l:expr) => {
        $l.map_err::<JsValue, _>(|err| js_sys::Error::new(&format!("{}", err)).into())?;
    };
}

/// Sets a [`panic hook`] for better error messages in NodeJS or web browser.
///
/// [`panic hook`]: https://crates.io/crates/console_error_panic_hook
#[wasm_bindgen(js_name = setWasmPanicHook)]
pub fn set_wasm_panic_hook() {
    panic::set_hook(Box::new(panic_hook));
}

/// Use `MessageFields` to attach user data to a [`Message`].
///
/// See [`crate::atomic::MessageFields`] for further documentation.
#[wasm_bindgen]
#[derive(Debug)]
pub struct MessageFields(MessageFieldsNonWasm);

#[wasm_bindgen]
impl MessageFields {
    /// Returns a `MessageFields` instance
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(MessageFieldsNonWasm::new())
    }

    /// Adds a new field to this `MessageFields` instance.
    ///
    /// Only `text` fields are currently supported and no schema validation is being done to make
    /// sure that only fields that are part of a schema can be added.
    pub fn add(&mut self, name: String, value: JsValue) -> Result<(), JsValue> {
        // @TODO: Add more types
        let field = match value.as_string() {
            Some(text) => Ok(MessageValue::Text(text)),
            None => Err(js_sys::Error::new(&format!("Invalid value type"))),
        }?;

        jserr!(self.0.add(&name, field));

        Ok(())
    }

    /// Returns this instance formatted for debugging
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}

/// Returns an encoded `create` message that creates an instance of the provided schema.
///
/// Use `create` messages by attaching them to an entry that you publish.
#[wasm_bindgen(js_name = encodeCreateMessage)]
pub fn encode_create_message(
    schema_hash: String,
    fields: MessageFields,
) -> Result<String, JsValue> {
    let schema = jserr!(Hash::new(&schema_hash));
    let message = jserr!(Message::new_create(schema, fields.0));
    let message_encoded = jserr!(MessageEncoded::try_from(&message));
    Ok(message_encoded.as_str().to_owned())
}

/// Returns an encoded `update` message that updates fields of a given instance.
///
/// Use `update` messages by attaching them to an entry that you publish.
#[wasm_bindgen(js_name = encodeUpdateMessage)]
pub fn encode_update_message(
    instance_id: String,
    schema_hash: String,
    fields: MessageFields,
) -> Result<String, JsValue> {
    let instance = jserr!(Hash::new(&instance_id));
    let schema = jserr!(Hash::new(&schema_hash));
    let message = jserr!(Message::new_update(schema, instance, fields.0));
    let message_encoded = jserr!(MessageEncoded::try_from(&message));
    Ok(message_encoded.as_str().to_owned())
}

/// Returns an encoded `delete` message that deletes a given instance.
///
/// Use `delete` messages by attaching them to an entry that you publish.
#[wasm_bindgen(js_name = encodeDeleteMessage)]
pub fn encode_delete_message(instance_id: String, schema_hash: String) -> Result<String, JsValue> {
    let instance = jserr!(Hash::new(&instance_id));
    let schema = jserr!(Hash::new(&schema_hash));
    let message = jserr!(Message::new_delete(schema, instance));
    let message_encoded = jserr!(MessageEncoded::try_from(&message));
    Ok(message_encoded.as_str().to_owned())
}

/// Return value of [`sign_encode_entry`] that holds the encoded entry and its hash
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignEncodeEntryResult {
    pub entry_encoded: String,
    pub entry_hash: String,
    pub message_hash: String,
}

/// Returns a signed and encoded entry that can be published to a p2panda node.
///
/// `entry_backlink_hash`, `entry_skiplink_hash`, `previous_seq_num` and `log_id` are obtained by
/// querying the `getEntryArguments` method of a p2panda node.
#[wasm_bindgen(js_name = signEncodeEntry)]
pub fn sign_encode_entry(
    key_pair: &KeyPair,
    encoded_message: String,
    entry_skiplink_hash: Option<String>,
    entry_backlink_hash: Option<String>,
    seq_num: i64,
    log_id: i64,
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

    // Convert to Message
    let message_encoded = jserr!(MessageEncoded::new(&encoded_message));
    let message = jserr!(Message::try_from(&message_encoded));

    // Create Entry instance
    let entry = jserr!(Entry::new(
        &LogId::new(log_id),
        Some(&message),
        skiplink_hash.as_ref(),
        backlink_hash.as_ref(),
        &seq_num,
    ));

    // Finally sign and encode entry
    let entry_signed = jserr!(sign_and_encode(&entry, &key_pair));

    // Serialize result to JSON
    let result = jserr!(wasm_bindgen::JsValue::from_serde(&SignEncodeEntryResult {
        entry_encoded: entry_signed.as_str().into(),
        entry_hash: entry_signed.hash().as_hex().into(),
        message_hash: message_encoded.hash().as_hex().into(),
    }));
    Ok(result)
}

/// Decodes an entry and optional message given their encoded form.
#[wasm_bindgen(js_name = decodeEntry)]
pub fn decode_entry(
    entry_encoded: String,
    message_encoded: Option<String>,
) -> Result<JsValue, JsValue> {
    // Convert encoded message
    let message_encoded = match message_encoded {
        Some(msg) => {
            let inner = jserr!(MessageEncoded::new(&msg));
            Some(inner)
        }
        None => None,
    };

    // Convert encoded entry
    let entry_signed = jserr!(EntrySigned::new(&entry_encoded));
    // Had to remove the jserr macro here to make this work, need to look at it again
    let entry: Entry = decode(&entry_signed, message_encoded.as_ref()).unwrap();

    // Serialize struct to JSON
    let result = jserr!(wasm_bindgen::JsValue::from_serde(&entry));
    Ok(result)
}
