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

    #[wasm_bindgen(js_name = publicKeyBytes)]
    pub fn public_key_bytes(&self) -> Box<[u8]> {
        Box::from(self.public.to_bytes())
    }

    #[wasm_bindgen(js_name = privateKeyBytes)]
    pub fn private_key_bytes(&self) -> Box<[u8]> {
        Box::from(self.private.to_bytes())
    }

    #[wasm_bindgen(js_name = publicKeyHex)]
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public.to_bytes())
    }

    #[wasm_bindgen(js_name = privateKeyHex)]
    pub fn private_key_hex(&self) -> String {
        hex::encode(self.private.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::KeyPair;
    use ed25519_dalek::{PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH};

    #[test]
    fn makes_keypair() {
        let key_pair = KeyPair::new();
        assert_eq!(key_pair.public_key_bytes().len(), PUBLIC_KEY_LENGTH);
        assert_eq!(key_pair.private_key_bytes().len(), SECRET_KEY_LENGTH);
        assert_eq!(key_pair.public_key_hex().len(), PUBLIC_KEY_LENGTH * 2);
        assert_eq!(key_pair.private_key_hex().len(), PUBLIC_KEY_LENGTH * 2);
    }
}
