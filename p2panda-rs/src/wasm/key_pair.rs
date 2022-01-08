// SPDX-License-Identifier: AGPL-3.0-or-later
use std::convert::TryFrom;

use ed25519_dalek::{PublicKey, Signature};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::identity::KeyPair as KeyPairNonWasm;
use crate::wasm::error::jserr;

/// Ed25519 key pair for authors to sign Bamboo entries with.
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
        let signature = self.0.sign(operation.as_bytes());
        hex::encode(signature.to_bytes())
    }

    /// Internal method to access non-wasm instance of `KeyPair`.
    pub(super) fn as_inner(&self) -> &KeyPairNonWasm {
        &self.0
    }
}

impl Default for KeyPair {
    fn default() -> Self {
        Self::new()
    }
}

/// Verify the integrity of a signed operation.
#[wasm_bindgen(js_name = verifySignature)]
pub fn verify_signature(
    public_key: String,
    byte_string: String,
    signature: String,
) -> Result<JsValue, JsValue> {
    // Convert all strings to byte sequences
    let public_key_bytes = jserr!(hex::decode(public_key));
    let unsigned_bytes = byte_string.as_bytes();
    let signature_bytes = jserr!(hex::decode(signature));

    // Create `PublicKey` and `Signature` instances from bytes
    let public_key = jserr!(PublicKey::from_bytes(&public_key_bytes));
    let signature = jserr!(Signature::try_from(&signature_bytes[..]));

    // Verify signature for given public key and operation
    match KeyPairNonWasm::verify(&public_key, unsigned_bytes, &signature) {
        Ok(_) => Ok(JsValue::TRUE),
        Err(_) => Ok(JsValue::FALSE),
    }
}
