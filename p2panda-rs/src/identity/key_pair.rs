// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::entry::Signature;
use crate::identity::error::KeyPairError;
use crate::identity::{PrivateKey, PublicKey};

/// Ed25519 key pair for authors to sign Bamboo entries with.
#[derive(Debug, Serialize, Deserialize)]
pub struct KeyPair(PrivateKey);

impl KeyPair {
    /// Generates a new key pair using the systems random number generator (CSPRNG) as a seed.
    ///
    /// This uses `getrandom` for random number generation internally. See [`getrandom`] crate for
    /// supported platforms.
    ///
    /// **WARNING:** Depending on the context this does not guarantee the random number generator
    /// to be cryptographically secure (eg. broken / hijacked browser or system implementations),
    /// so make sure to only run this in trusted environments.
    ///
    /// [`getrandom`]: https://docs.rs/getrandom/0.2.1/getrandom/
    ///
    /// ## Example
    ///
    /// ```
    /// # extern crate p2panda_rs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use p2panda_rs::identity::KeyPair;
    ///
    /// // Generate new Ed25519 key pair
    /// let key_pair = KeyPair::new();
    ///
    /// println!("{:?}", key_pair.public_key());
    /// println!("{:?}", key_pair.private_key());
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Self {
        Self(PrivateKey::new())
    }

    /// Derives a key pair from a private key.
    ///
    /// **WARNING:** "Absolutely no validation is done on the key. If you give this function bytes
    /// which do not represent a valid point, or which do not represent corresponding parts of the
    /// key, then your KeyPair will be broken and it will be your fault." See [`ed25519-dalek`]
    /// crate.
    ///
    /// [`ed25519-dalek`]: https://docs.rs/ed25519-dalek/1.0.1/ed25519_dalek/struct.Keypair.html#warning
    ///
    /// ## Example
    ///
    /// ```
    /// # extern crate p2panda_rs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use p2panda_rs::identity::KeyPair;
    ///
    /// // Generate new Ed25519 key pair
    /// let key_pair = KeyPair::new();
    ///
    /// // Derive a key pair from a private key
    /// let key_pair_derived = KeyPair::from_private_key(key_pair.private_key())?;
    ///
    /// assert_eq!(key_pair.public_key(), key_pair_derived.public_key());
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_private_key(private_key: &PrivateKey) -> Result<Self, KeyPairError> {
        Ok(Self(private_key.to_owned()))
    }

    /// Derives a key pair from a private key (encoded as hex string for better handling in browser
    /// contexts).
    pub fn from_private_key_str(private_key: &str) -> Result<Self, KeyPairError> {
        let secret_key_bytes = hex::decode(private_key)?;
        let secret_key = PrivateKey::from_bytes(&secret_key_bytes)?;
        Self::from_private_key(&secret_key)
    }

    /// Returns the public half of the key pair.
    pub fn public_key(&self) -> PublicKey {
        self.0.public_key()
    }

    /// Returns the private half of the key pair.
    pub fn private_key(&self) -> PrivateKey {
        self.0.clone()
    }

    /// Sign any data using this key pair.
    ///
    /// ## Example
    ///
    /// ```
    /// # extern crate p2panda_rs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use p2panda_rs::identity::KeyPair;
    ///
    /// // Generate new Ed25519 key pair
    /// let key_pair = KeyPair::new();
    ///
    /// // Sign bytes with this key pair
    /// let bytes = b"test";
    /// let signature = key_pair.sign(bytes);
    ///
    /// assert!(KeyPair::verify(&key_pair.public_key(), bytes, &signature).is_ok());
    /// # Ok(())
    /// # }
    /// ```
    pub fn sign(&self, bytes: &[u8]) -> Signature {
        self.0.sign(bytes)
    }
}

impl Default for KeyPair {
    fn default() -> Self {
        KeyPair::new()
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH};

    use super::KeyPair;

    #[test]
    fn makes_keypair() {
        let key_pair = KeyPair::new();
        assert_eq!(key_pair.public_key().to_bytes().len(), PUBLIC_KEY_LENGTH);
        assert_eq!(key_pair.private_key().to_bytes().len(), SECRET_KEY_LENGTH);
    }

    #[test]
    fn key_pair_from_private_key() {
        let key_pair = KeyPair::new();
        let key_pair2 = KeyPair::from_private_key(&key_pair.private_key()).unwrap();
        assert_eq!(key_pair.public_key(), key_pair2.public_key());
    }
}
