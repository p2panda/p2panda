use bamboo_rs_core::yamf_hash::new_blake2b;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError, ValidationErrors};
use yamf_hash::{YamfHash, MAX_YAMF_HASH_SIZE};

use crate::error::{Result, ValidationResult};

/// Hash of entry or message encoded as hex string.
///
/// This uses the BLAKE2b algorithm wrapped in YAMF "Yet-Another-Multi-Format" according to the
/// Bamboo specification.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hash(String);

impl Hash {
    /// Validates and returns new hash instance when correct.
    pub fn new(value: &str) -> Result<Self> {
        let hash = Self(String::from(value));
        hash.validate()?;
        Ok(hash)
    }

    /// Hashes byte data and returns it as hash instance.
    #[allow(dead_code)]
    pub fn from_bytes(value: Vec<u8>) -> Result<Self> {
        // Generate Blake2b hash
        let blake2b_hash = new_blake2b(&value);

        // Wrap hash in YAMF container format
        let mut bytes = Vec::new();
        blake2b_hash.encode_write(&mut bytes)?;

        // Encode bytes as hex string
        let hex_str = hex::encode(&bytes);

        Ok(Self(hex_str))
    }

    /// Returns hash as bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Unwrap as we already validated the hash
        hex::decode(&self.0).unwrap()
    }

    /// Returns hash as hex string.
    #[allow(dead_code)]
    pub fn to_hex(&self) -> &str {
        self.0.as_str()
    }
}

impl Validate for Hash {
    fn validate(&self) -> ValidationResult {
        let mut errors = ValidationErrors::new();

        // Check if hash is a hex string
        match hex::decode(self.0.to_owned()) {
            Ok(bytes) => {
                // Check if length is correct
                if bytes.len() != MAX_YAMF_HASH_SIZE {
                    errors.add("hash", ValidationError::new("invalid hash length"));
                }

                // Check if YAMF BLAKE2b hash is valid
                match YamfHash::<&[u8]>::decode(&bytes) {
                    Ok((YamfHash::Blake2b(_), _)) => {}
                    _ => {
                        errors.add(
                            "hash",
                            ValidationError::new("can't decode YAMF BLAKE2b hash"),
                        );
                    }
                }
            }
            Err(_) => {
                errors.add("hash", ValidationError::new("invalid hex string"));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl PartialEq for Hash {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[cfg(test)]
mod tests {
    use super::Hash;

    #[test]
    fn validate() {
        assert!(Hash::new("abcdefg").is_err());
        assert!(Hash::new("112233445566ff").is_err());
        assert!(
            Hash::new("01234567812345678123456781234567812345678123456781234567812345678").is_err()
        );
        assert!(
            Hash::new("004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8").is_ok()
        );
    }

    #[test]
    fn from_bytes() {
        assert_eq!(Hash::from_bytes(vec![1, 2, 3]).unwrap(), Hash::new("0040cf94f6d605657e90c543b0c919070cdaaf7209c5e1ea58acb8f3568fa2114268dc9ac3bafe12af277d286fce7dc59b7c0c348973c4e9dacbe79485e56ac2a702").unwrap());
    }
}
