//! Methods exported for WebAssembly targets.
//!
//! Wrappers for these methods are available in [p2panda-js], which allows idiomatic usage of
//! `p2panda-rs` in a Javascript/Typescript environment.
//!
//! [p2panda-js]: https://github.com/p2panda/p2panda/tree/main/p2panda-js
use std::convert::{TryFrom, TryInto};
use std::panic;

use console_error_panic_hook::hook as panic_hook;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::entry::{decode_entry as decode, sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::message::{
    Message, MessageEncoded, MessageFields as MessageFieldsNonWasm, MessageValue,
};

// Converts any Rust Error type into js_sys:Error while keeping its error message. This helps
// propagating errors similar like we do in Rust but in WebAssembly contexts.
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
    /// Returns a `MessageFields` instance.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(MessageFieldsNonWasm::new())
    }

    /// Adds a field with a value and a given value type.
    ///
    /// The type is defined by a simple string, similar to an enum. Since Rust enums can not (yet)
    /// be exported via wasm-bindgen we have to do it like this. Possible type values are "text"
    /// (String), "boolean" (Boolean), "float" (Number), "relation" (String representing a
    /// hex-encoded hash) and "integer" (Number).
    ///
    /// This method will throw an error when the field was already set, an invalid type value got
    /// passed or when the value does not reflect the given type.
    #[wasm_bindgen()]
    pub fn add(&mut self, name: String, value_type: String, value: JsValue) -> Result<(), JsValue> {
        match &value_type[..] {
            "text" => {
                let value_str = jserr!(value.as_string().ok_or("Invalid string value"));
                jserr!(self.0.add(&name, MessageValue::Text(value_str)));
                Ok(())
            }
            "boolean" => {
                let value_bool = jserr!(value.as_bool().ok_or("Invalid boolean value"));
                jserr!(self.0.add(&name, MessageValue::Boolean(value_bool)));
                Ok(())
            }
            "integer" => {
                let value_int = jserr!(value.as_f64().ok_or("Invalid integer value")) as i64;
                jserr!(self.0.add(&name, MessageValue::Integer(value_int)));
                Ok(())
            }
            "float" => {
                let value_float = jserr!(value.as_f64().ok_or("Invalid float value"));
                jserr!(self.0.add(&name, MessageValue::Float(value_float)));
                Ok(())
            }
            "relation" => {
                let value_str = jserr!(value.as_string().ok_or("Invalid string value"));
                let hash = jserr!(Hash::new(&value_str));
                jserr!(self.0.add(&name, MessageValue::Relation(hash)));
                Ok(())
            }
            _ => Err(js_sys::Error::new("Unknown type value").into()),
        }
    }

    /// Removes an existing field from this `MessageFields` instance.
    ///
    /// This might throw an error when trying to remove an inexistent field.
    #[wasm_bindgen()]
    pub fn remove(&mut self, name: String) -> Result<(), JsValue> {
        jserr!(self.0.remove(&name));
        Ok(())
    }

    /// Returns field of this `MessageFields` instance when existing.
    ///
    /// When trying to access an integer field the method might throw an error when the internal
    /// value is larger than an i32 number. The wasm API will use i32 numbers in JavaScript
    /// contexts instead of i64 / BigInt as long as BigInt support is not given in Safari on MacOS
    /// and iOS.
    #[wasm_bindgen()]
    pub fn get(&mut self, name: String) -> Result<JsValue, JsValue> {
        match self.0.get(&name) {
            Some(MessageValue::Boolean(value)) => Ok(JsValue::from_bool(value.to_owned())),
            Some(MessageValue::Text(value)) => Ok(JsValue::from_str(value)),
            Some(MessageValue::Relation(value)) => Ok(JsValue::from_str(&value.as_str())),
            Some(MessageValue::Float(value)) => Ok(JsValue::from_f64(value.to_owned())),
            Some(MessageValue::Integer(value)) => {
                // Downcast i64 to i32 and throw error when value too large
                let converted: i32 = jserr!(value.to_owned().try_into());
                Ok(converted.into())
            }
            None => Ok(JsValue::NULL),
        }
    }

    /// Returns the number of fields in this instance.
    #[wasm_bindgen(js_name = length)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns this instance formatted for debugging.
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
///
/// `previous_seq_num` and `log_id` are `i32` parameters even though they have 64 bits in the
/// bamboo spec. Webkit doesn't support `BigInt` so it can't handle those large values.
#[wasm_bindgen(js_name = signEncodeEntry)]
pub fn sign_encode_entry(
    key_pair: &KeyPair,
    encoded_message: String,
    entry_skiplink_hash: Option<String>,
    entry_backlink_hash: Option<String>,
    seq_num: i32,
    log_id: i32,
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
    let seq_num = jserr!(SeqNum::new(seq_num.into()));

    // Convert to Message
    let message_encoded = jserr!(MessageEncoded::new(&encoded_message));
    let message = jserr!(Message::try_from(&message_encoded));

    // Create Entry instance
    let entry = jserr!(Entry::new(
        &LogId::new(log_id.into()),
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
        entry_hash: entry_signed.hash().as_str().into(),
        message_hash: message_encoded.hash().as_str().into(),
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
    let entry: Entry = jserr!(decode(&entry_signed, message_encoded.as_ref()));

    // Serialize struct to JSON
    let result = jserr!(wasm_bindgen::JsValue::from_serde(&entry));
    Ok(result)
}
