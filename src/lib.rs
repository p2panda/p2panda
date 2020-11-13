extern crate wasm_bindgen;
extern crate console_error_panic_hook;

use ed25519_dalek::{Keypair as DalekKeypair, SecretKey, PublicKey, PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH};
use std::panic;
use rand::rngs::OsRng;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn set_panic_hook() {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
}

#[wasm_bindgen]
pub fn hello() -> String {
    "Hallo, hier ist alles schön".into()
}


#[wasm_bindgen]
pub struct PandaKeyPair {
    public: PublicKey,
    private: SecretKey
}

#[wasm_bindgen]
impl PandaKeyPair {
    #[wasm_bindgen(constructor)]
    pub fn new () -> PandaKeyPair {
        let mut csprng: OsRng = OsRng {};
        let key_pair: DalekKeypair = DalekKeypair::generate(&mut csprng);
        println!("{:?}", key_pair.to_bytes().to_vec());
        PandaKeyPair {
            public: key_pair.public,
            private: key_pair.secret
        }
    }

    #[wasm_bindgen]
    pub fn public_key_bytes(&self) -> Vec<u8> {
        Vec::from(&self.public.as_bytes()[..])
    }

    #[wasm_bindgen]
    pub fn private_key_bytes(&self) -> Vec<u8> {
        Vec::from(&self.private.as_bytes()[..])
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(hello(), "Hallo, hier ist alles schön");
    }

    #[test]
    fn makes_keypair() {
        let key_pair = PandaKeyPair::new();
        assert_eq!(key_pair.public_key_bytes().len(), PUBLIC_KEY_LENGTH);
        assert_eq!(key_pair.private_key_bytes().len(), SECRET_KEY_LENGTH);
    }
}
