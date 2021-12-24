// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods exported for WebAssembly targets.
//!
//! Wrappers for these methods are available in [p2panda-js], which allows idiomatic usage of
//! `p2panda-rs` in a Javascript/Typescript environment.
//!
//! [p2panda-js]: https://github.com/p2panda/p2panda/tree/main/p2panda-js
use std::convert::{TryFrom, TryInto};
use std::panic;

use crate::entry::{decode_entry as decode, sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair as KeyPairNonWasm;
use crate::operation::{
    Operation, OperationEncoded, OperationFields as OperationFieldsNonWasm, OperationValue,
};
use console_error_panic_hook::hook as panic_hook;
use ed25519_dalek::{PublicKey, Signature};
use js_sys::Array;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

// Converts any Rust Error type into js_sys:Error while keeping its error message. This helps
// propagating errors similar like we do in Rust but in WebAssembly contexts. It is possible to
// optionally use a custom error message when required.
macro_rules! jserr {
    // Convert error to js_sys::Error with original error message
    ($l:expr) => {
        $l.map_err::<JsValue, _>(|err| js_sys::Error::new(&format!("{}", err)).into())?
    };

    // Convert error to js_sys::Error with custom error message
    ($l:expr, $err:expr) => {
        $l.map_err::<JsValue, _>(|_| js_sys::Error::new(&format!("{:?}", $err)).into())?
    };
}

/// Sets a [`panic hook`] for better error messages in NodeJS or web browser.
///
/// [`panic hook`]: https://crates.io/crates/console_error_panic_hook
#[wasm_bindgen(js_name = setWasmPanicHook)]
pub fn set_wasm_panic_hook() {
    panic::set_hook(Box::new(panic_hook));
}

/// Ed25519 key pair for authors to sign bamboo entries with.
#[wasm_bindgen]
#[derive(Debug)]
pub struct KeyPair(KeyPairNonWasm);

#[wasm_bindgen]
impl KeyPair {
    /// Generates a new key pair using the browsers random number generator as a seed.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(KeyPairNonWasm::new())
    }

    /// Derives a key pair from a private key, encoded as hex string for better handling in browser
    /// contexts.
    #[wasm_bindgen(js_name = fromPrivateKey)]
    pub fn from_private_key(private_key: String) -> Result<KeyPair, JsValue> {
        let key_pair_inner = jserr!(KeyPairNonWasm::from_private_key_str(&private_key));
        Ok(KeyPair(key_pair_inner))
    }

    /// Returns the public half of the key pair, encoded as a hex string.
    #[wasm_bindgen(js_name = publicKey)]
    pub fn public_key(&self) -> String {
        hex::encode(self.0.public_key().to_bytes())
    }

    /// Returns the private half of the key pair, encoded as a hex string.
    #[wasm_bindgen(js_name = privateKey)]
    pub fn private_key(&self) -> String {
        hex::encode(self.0.private_key().to_bytes())
    }

    /// Sign an operation using this key pair, returns signature encoded as a hex string.
    #[wasm_bindgen]
    pub fn sign(&self, operation: String) -> String {
        let signature = self.0.sign(&operation.as_bytes());
        hex::encode(signature.to_bytes())
    }

    /// Internal method to access non-wasm instance of `KeyPair`.
    pub(crate) fn as_inner(&self) -> &KeyPairNonWasm {
        &self.0
    }
}

/// Verify the integrity of a signed operation.
#[wasm_bindgen(js_name = verifySignature)]
pub fn verify_signature(
    public_key: String,
    operation: String,
    signature: String,
) -> Result<JsValue, JsValue> {
    // Convert all strings to byte sequences
    let public_key_bytes = jserr!(hex::decode(public_key));
    let operation_bytes = operation.as_bytes();
    let signature_bytes = jserr!(hex::decode(signature));

    // Create `PublicKey` and `Signature` instances from bytes
    let public_key = jserr!(PublicKey::from_bytes(&public_key_bytes));
    let signature = jserr!(Signature::try_from(&signature_bytes[..]));

    // Verify signature for given public key and operation
    match KeyPairNonWasm::verify(&public_key, &operation_bytes, &signature) {
        Ok(_) => Ok(JsValue::TRUE),
        Err(_) => Ok(JsValue::FALSE),
    }
}

/// Use `OperationFields` to attach application data to a [`Operation`].
///
/// See [`crate::atomic::OperationFields`] for further documentation.
#[wasm_bindgen]
#[derive(Debug)]
pub struct OperationFields(OperationFieldsNonWasm);

#[wasm_bindgen]
impl OperationFields {
    /// Returns a `OperationFields` instance.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(OperationFieldsNonWasm::new())
    }

    /// Adds a field with a value and a given value type.
    ///
    /// The type is defined by a simple string, similar to an enum. Since Rust enums can not (yet)
    /// be exported via wasm-bindgen we have to do it like this. Possible type values are "str"
    /// (String), "bool" (Boolean), "float" (Number), "relation" (String representing a hex-encoded
    /// hash) and "int" (Number).
    ///
    /// This method will throw an error when the field was already set, an invalid type value got
    /// passed or when the value does not reflect the given type.
    #[wasm_bindgen]
    pub fn add(&mut self, name: String, value_type: String, value: JsValue) -> Result<(), JsValue> {
        match &value_type[..] {
            "str" => {
                let value_str = jserr!(value.as_string().ok_or("Invalid string value"));
                jserr!(self.0.add(&name, OperationValue::Text(value_str)));
                Ok(())
            }
            "bool" => {
                let value_bool = jserr!(value.as_bool().ok_or("Invalid boolean value"));
                jserr!(self.0.add(&name, OperationValue::Boolean(value_bool)));
                Ok(())
            }
            "int" => {
                // Bear in mind JavaScript does not represent numbers as integers, all numbers
                // are represented as floats therefore if a float is passed incorrectly it will
                // simply be cast to an int.
                // See: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number
                let value_int = jserr!(value.as_f64().ok_or("Invalid integer value")) as i64;
                jserr!(self.0.add(&name, OperationValue::Integer(value_int)));
                Ok(())
            }
            "float" => {
                let value_float = jserr!(value.as_f64().ok_or("Invalid float value"));
                jserr!(self.0.add(&name, OperationValue::Float(value_float)));
                Ok(())
            }
            "relation" => {
                let value_str = jserr!(value.as_string().ok_or("Invalid string value"));
                let hash = jserr!(Hash::new(&value_str));
                jserr!(self.0.add(&name, OperationValue::Relation(hash)));
                Ok(())
            }
            _ => Err(js_sys::Error::new("Unknown type value").into()),
        }
    }

    /// Removes an existing field from this `OperationFields` instance.
    ///
    /// This might throw an error when trying to remove an inexistent field.
    #[wasm_bindgen]
    pub fn remove(&mut self, name: String) -> Result<(), JsValue> {
        jserr!(self.0.remove(&name));
        Ok(())
    }

    /// Returns field of this `OperationFields` instance when existing.
    ///
    /// When trying to access an integer field the method might throw an error when the internal
    /// value is larger than an i32 number. The wasm API will use i32 numbers in JavaScript
    /// contexts instead of i64 / BigInt as long as BigInt support is not given in Safari on MacOS
    /// and iOS.
    #[wasm_bindgen]
    pub fn get(&mut self, name: String) -> Result<JsValue, JsValue> {
        match self.0.get(&name) {
            Some(OperationValue::Boolean(value)) => Ok(JsValue::from_bool(value.to_owned())),
            Some(OperationValue::Text(value)) => Ok(JsValue::from_str(value)),
            Some(OperationValue::Relation(value)) => Ok(JsValue::from_str(&value.as_str())),
            Some(OperationValue::Float(value)) => Ok(JsValue::from_f64(value.to_owned())),
            Some(OperationValue::Integer(value)) => {
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

/// Returns an encoded `create` operation that creates a document of the provided schema.
///
/// Use `create` operations by attaching them to an entry that you publish.
#[wasm_bindgen(js_name = encodeCreateOperation)]
pub fn encode_create_operation(
    schema_hash: String,
    fields: OperationFields,
) -> Result<String, JsValue> {
    let schema = jserr!(Hash::new(&schema_hash));
    let operation = jserr!(Operation::new_create(schema, fields.0));
    let operation_encoded = jserr!(OperationEncoded::try_from(&operation));
    Ok(operation_encoded.as_str().to_owned())
}

/// Returns an encoded `update` operation that updates fields of a given document.
///
/// Use `update` operations by attaching them to an entry that you publish.
#[wasm_bindgen(js_name = encodeUpdateOperation)]
pub fn encode_update_operation(
    document_id: String,
    schema_hash: String,
    previous_operations: JsValue,
    fields: OperationFields,
) -> Result<String, JsValue> {
    let document = jserr!(Hash::new(&document_id));
    let schema = jserr!(Hash::new(&schema_hash));
    // decode JsValue into vector of strings
    let prev_op_strings: Vec<String> = jserr!(previous_operations.into_serde());
    // create hashes from strings and collect wrapped in a result
    let prev_op_result: Result<Vec<Hash>, _> = prev_op_strings
        .iter()
        .map(|prev_op| Hash::new(&prev_op))
        .collect();
    // unwrap with jserr! macro
    let previous = jserr!(prev_op_result);
    let operation = jserr!(Operation::new_update(schema, document, previous, fields.0));
    let operation_encoded = jserr!(OperationEncoded::try_from(&operation));
    Ok(operation_encoded.as_str().to_owned())
}

/// Returns an encoded `delete` operation that deletes a given document.
///
/// Use `delete` operations by attaching them to an entry that you publish.
#[wasm_bindgen(js_name = encodeDeleteOperation)]
pub fn encode_delete_operation(
    document_id: String,
    schema_hash: String,
    previous_operations: Array,
) -> Result<String, JsValue> {
    let document = jserr!(Hash::new(&document_id));
    let schema = jserr!(Hash::new(&schema_hash));
    // decode JsValue into vector of strings
    let prev_op_strings: Vec<String> = jserr!(previous_operations.into_serde());
    // create hashes from strings and collect wrapped in a result
    let prev_op_result: Result<Vec<Hash>, _> = prev_op_strings
        .iter()
        .map(|prev_op| Hash::new(&prev_op))
        .collect();
    // unwrap with jserr! macro
    let previous = jserr!(prev_op_result);
    let operation = jserr!(Operation::new_delete(schema, document, previous));
    let operation_encoded = jserr!(OperationEncoded::try_from(&operation));
    Ok(operation_encoded.as_str().to_owned())
}

/// Return value of [`sign_encode_entry`] that holds the encoded entry and its hash
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignEncodeEntryResult {
    pub entry_encoded: String,
    pub entry_hash: String,
    pub operation_hash: String,
}

/// Returns a signed and encoded entry that can be published to a p2panda node.
///
/// `entry_backlink_hash`, `entry_skiplink_hash`, `seq_num` and `log_id` are obtained by querying
/// the `getEntryArguments` method of a p2panda node.
///
/// `seq_num` and `log_id` are `i32` parameters even though they have 64 bits in the bamboo spec.
/// Webkit doesn't support `BigInt` so it can't handle those large values.
#[wasm_bindgen(js_name = signEncodeEntry)]
pub fn sign_encode_entry(
    key_pair: &KeyPair,
    encoded_operation: String,
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

    // Convert to Operation
    let operation_encoded = jserr!(OperationEncoded::new(&encoded_operation));
    let operation = jserr!(Operation::try_from(&operation_encoded));

    // Create Entry instance
    let entry = jserr!(Entry::new(
        &LogId::new(log_id.into()),
        Some(&operation),
        skiplink_hash.as_ref(),
        backlink_hash.as_ref(),
        &seq_num,
    ));

    // Finally sign and encode entry
    let entry_signed = jserr!(sign_and_encode(&entry, key_pair.as_inner()));

    // Serialize result to JSON
    let result = jserr!(wasm_bindgen::JsValue::from_serde(&SignEncodeEntryResult {
        entry_encoded: entry_signed.as_str().into(),
        entry_hash: entry_signed.hash().as_str().into(),
        operation_hash: operation_encoded.hash().as_str().into(),
    }));
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

    // Serialize struct to JSON
    let result = jserr!(wasm_bindgen::JsValue::from_serde(&entry));
    Ok(result)
}
