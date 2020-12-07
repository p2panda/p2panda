use console_error_panic_hook::hook as panic_hook;
use ed25519_dalek::{Keypair as Ed25519Keypair, PublicKey, SecretKey};
use rand::rngs::OsRng;
use std::panic;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(js_name = setWasmPanicHook)]
pub fn set_wasm_panic_hook() {
    panic::set_hook(Box::new(panic_hook));
}

#[wasm_bindgen]
pub struct KeyPair {
    public: PublicKey,
    private: SecretKey,
}

#[wasm_bindgen]
impl KeyPair {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        let mut csprng: OsRng = OsRng {};
        let key_pair = Ed25519Keypair::generate(&mut csprng);

        Self {
            public: key_pair.public,
            private: key_pair.secret,
        }
    }

    #[wasm_bindgen(js_name = fromPrivateKey)]
    pub fn from_private_key(private_key: String) -> Self {
        let bytes = hex::decode(private_key).unwrap();
        let secret_key = SecretKey::from_bytes(&bytes).unwrap();
        let public_key: PublicKey = (&secret_key).into();

        Self {
            public: public_key,
            private: secret_key,
        }
    }

    #[wasm_bindgen(js_name = publicKey)]
    pub fn public_key(&self) -> String {
        hex::encode(self.public.to_bytes())
    }

    #[wasm_bindgen(js_name = privateKey)]
    pub fn private_key(&self) -> String {
        hex::encode(self.private.to_bytes())
    }

    #[wasm_bindgen(js_name = publicKeyBytes)]
    pub fn public_key_bytes(&self) -> Box<[u8]> {
        Box::from(self.public.to_bytes())
    }

    #[wasm_bindgen(js_name = privateKeyBytes)]
    pub fn private_key_bytes(&self) -> Box<[u8]> {
        Box::from(self.private.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::KeyPair;
    use ed25519_dalek::{PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH};

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
}
