use ed25519_dalek::Keypair as Ed25519Keypair;
use rand::rngs::OsRng;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

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

    /// Derives a key pair from hex-encoded serialized representation
    ///
    /// WARNING: "Absolutely no validation is done on the key. If you give this function bytes
    /// which do not represent a valid point, or which do not represent corresponding parts of the
    /// key, then your Keypair will be broken and it will be your fault."
    /// https://docs.rs/ed25519-dalek/1.0.1/ed25519_dalek/struct.Keypair.html#warning
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = fromHex))]
    pub fn from_hex(encoded_key_pair: String) -> Self {
        let bytes = hex::decode(encoded_key_pair).unwrap();
        let key_pair = Ed25519Keypair::from_bytes(&bytes).unwrap();

        Self(key_pair)
    }

    /// Returns the key pair, including secret key, encoded as a hex string.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = toHex))]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0.to_bytes())
    }

    /// Returns the key pair as a byte representation
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = toBytes))]
    pub fn to_bytes(&self) -> Box<[u8]> {
        Box::from(self.0.to_bytes())
    }

    /// Returns the public half of the key pair, encoded as a hex string.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = publicKey))]
    pub fn public_key(&self) -> String {
        hex::encode(self.0.public.as_bytes())
    }

    /// Returns the public half of the key pair.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = publicKeyBytes))]
    pub fn public_key_bytes(&self) -> Box<[u8]> {
        Box::from(self.0.public.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{KEYPAIR_LENGTH, PUBLIC_KEY_LENGTH};

    use super::KeyPair;

    #[test]
    fn makes_keypair() {
        let key_pair = KeyPair::new();
        assert_eq!(key_pair.public_key().len(), PUBLIC_KEY_LENGTH * 2);
        assert_eq!(key_pair.public_key_bytes().len(), PUBLIC_KEY_LENGTH);
        assert_eq!(key_pair.to_hex().len(), KEYPAIR_LENGTH * 2);
        assert_eq!(key_pair.to_bytes().len(), KEYPAIR_LENGTH);
    }

    #[test]
    fn key_pair_from_hex_encoding() {
        let key_pair = KeyPair::new();
        let key_pair2 = KeyPair::from_hex(key_pair.to_hex());
        assert_eq!(key_pair.public_key_bytes(), key_pair2.public_key_bytes());
        assert_eq!(key_pair.to_bytes(), key_pair2.to_bytes());
    }
}
