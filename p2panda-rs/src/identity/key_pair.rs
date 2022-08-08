// SPDX-License-Identifier: AGPL-3.0-or-later

use ed25519_dalek::{Keypair as Ed25519Keypair, PublicKey, SecretKey, Signature, Signer, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::identity::error::KeyPairError;

/// Ed25519 key pair for authors to sign Bamboo entries with.
#[derive(Debug, Serialize, Deserialize)]
pub struct KeyPair(Ed25519Keypair);

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
        let mut csprng: OsRng = OsRng {};
        let key_pair = Ed25519Keypair::generate(&mut csprng);
        Self(key_pair)
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
    pub fn from_private_key(private_key: &SecretKey) -> Result<Self, KeyPairError> {
        // Derive public part from secret part
        let public_key: PublicKey = private_key.into();

        // Assemble key pair from both parts
        let bytes = [private_key.to_bytes(), public_key.to_bytes()].concat();
        let key_pair = Ed25519Keypair::from_bytes(&bytes)?;

        Ok(KeyPair(key_pair))
    }

    /// Derives a key pair from a private key (encoded as hex string for better handling in browser
    /// contexts).
    pub fn from_private_key_str(private_key: &str) -> Result<Self, KeyPairError> {
        let secret_key_bytes = hex::decode(private_key)?;
        let secret_key = SecretKey::from_bytes(&secret_key_bytes)?;
        Self::from_private_key(&secret_key)
    }

    /// Returns the public half of the key pair.
    pub fn public_key(&self) -> &PublicKey {
        &self.0.public
    }

    /// Returns the private half of the key pair.
    pub fn private_key(&self) -> &SecretKey {
        &self.0.secret
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

    /// Verify the integrity of signed data.
    pub fn verify(
        public_key: &PublicKey,
        bytes: &[u8],
        signature: &Signature,
    ) -> Result<(), KeyPairError> {
        public_key.verify(bytes, signature)?;
        Ok(())
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
        let key_pair2 = KeyPair::from_private_key(key_pair.private_key()).unwrap();
        assert_eq!(key_pair.public_key(), key_pair2.public_key());
    }

    #[test]
    fn signing() {
        let key_pair = KeyPair::new();
        let bytes = b"test";
        let signature = key_pair.sign(bytes);
        assert!(KeyPair::verify(key_pair.public_key(), bytes, &signature).is_ok());

        // Invalid data
        assert!(KeyPair::verify(key_pair.public_key(), b"not test", &signature).is_err());

        // Invalid public key
        let key_pair_2 = KeyPair::new();
        assert!(KeyPair::verify(key_pair_2.public_key(), bytes, &signature).is_err());
    }
}
