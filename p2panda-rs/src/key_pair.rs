use std::convert::TryFrom;

#[cfg(not(target_arch = "wasm32"))]
use ed25519_dalek::SignatureError;
use ed25519_dalek::{Keypair as Ed25519Keypair, PublicKey, SecretKey, Signature, Signer};
#[cfg(target_arch = "wasm32")]
use js_sys::Error as JsError;
use rand::rngs::OsRng;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

/// Ed25519 key pair for authors to sign bamboo entries with.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Debug)]
pub struct KeyPair(Ed25519Keypair);

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl KeyPair {
    /// Generates a new key pair using the systems random number generator (CSPRNG) as a seed.
    ///
    /// For random number generation this uses `getrandom` internally. See the following link for
    /// supported platforms: https://docs.rs/getrandom/0.2.1/getrandom/.
    ///
    /// WARNING: Depending on the context this does not guarantee the random number generator
    /// to be cryptographically secure (eg. broken / hijacked browser or system implementations),
    /// so make sure to only run this in trusted environments.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> Self {
        let mut csprng: OsRng = OsRng {};
        let key_pair = Ed25519Keypair::generate(&mut csprng);
        Self(key_pair)
    }

    /// Derives a key pair from a private key (encoded as hex string).
    ///
    /// WARNING: "Absolutely no validation is done on the key. If you give this function bytes
    /// which do not represent a valid point, or which do not represent corresponding parts of the
    /// key, then your Keypair will be broken and it will be your fault."
    /// https://docs.rs/ed25519-dalek/1.0.1/ed25519_dalek/struct.Keypair.html#warning
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = fromPrivateKey))]
    pub fn from_private_key(private_key: String) -> Self {
        // Decode private key
        let secret_key_bytes = hex::decode(private_key).unwrap();
        let secret_key = SecretKey::from_bytes(&secret_key_bytes).unwrap();

        // Derive public part from secret part
        let public_key: PublicKey = (&secret_key).into();

        // Assemble key pair from both parts
        let bytes = [secret_key.to_bytes(), public_key.to_bytes()].concat();
        let key_pair = Ed25519Keypair::from_bytes(&bytes).unwrap();

        Self(key_pair)
    }

    /// Returns the public half of the key pair, encoded as a hex string.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = publicKey))]
    pub fn public_key(&self) -> String {
        hex::encode(self.0.public.to_bytes())
    }

    /// Returns the private half of the key pair, encoded as a hex string.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = privateKey))]
    pub fn private_key(&self) -> String {
        hex::encode(self.0.secret.to_bytes())
    }

    /// Returns the public half of the key pair.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = publicKeyBytes))]
    pub fn public_key_bytes(&self) -> Box<[u8]> {
        Box::from(self.0.public.to_bytes())
    }

    /// Returns the private half of the key pair.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = privateKeyBytes))]
    pub fn private_key_bytes(&self) -> Box<[u8]> {
        Box::from(self.0.secret.to_bytes())
    }

    /// Sign a message using this key pair.
    pub fn sign(&self, message: &[u8]) -> Box<[u8]> {
        Box::from(self.0.sign(message).to_bytes())
    }

    /// Verify a signature for a message.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), SignatureError> {
        self.0.verify(message, &Signature::try_from(signature)?)
    }

    /// Verify a signature for a message.
    #[cfg(target_arch = "wasm32")]
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<bool, JsValue> {
        match self
            .0
            .verify(message, &Signature::try_from(signature).unwrap())
        {
            Ok(_) => Ok(true),
            Err(_) => Err(JsError::new("invalid signature for this key pair").into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH};

    use super::KeyPair;

    #[test]
    fn makes_keypair() {
        let key_pair = KeyPair::new();
        assert_eq!(key_pair.public_key().len(), PUBLIC_KEY_LENGTH * 2);
        assert_eq!(key_pair.private_key().len(), PUBLIC_KEY_LENGTH * 2);
        assert_eq!(key_pair.public_key_bytes().len(), PUBLIC_KEY_LENGTH);
        assert_eq!(key_pair.private_key_bytes().len(), SECRET_KEY_LENGTH);
    }

    #[test]
    fn key_pair_from_private_key() {
        let key_pair = KeyPair::new();
        let key_pair2 = KeyPair::from_private_key(key_pair.private_key());
        assert_eq!(key_pair.public_key_bytes(), key_pair2.public_key_bytes());
        assert_eq!(key_pair.private_key_bytes(), key_pair2.private_key_bytes());
    }

    #[test]
    fn signing() {
        let key_pair = KeyPair::new();
        let message = b"test";
        let signature = key_pair.sign(message);
        assert!(key_pair.verify(message, &signature).is_ok());
        assert!(key_pair.verify(b"not test", &signature).is_err());

        let key_pair2 = KeyPair::new();
        assert!(key_pair2.verify(message, &signature).is_err());
    }
}
