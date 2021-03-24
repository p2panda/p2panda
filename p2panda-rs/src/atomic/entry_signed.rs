use std::convert::{TryFrom, TryInto};

use anyhow::bail;
use arrayvec::ArrayVec;
use bamboo_rs_core::entry::MAX_ENTRY_SIZE;
use bamboo_rs_core::{Entry as BambooEntry, Signature as BambooSignature};
use ed25519_dalek::PublicKey;
use thiserror::Error;

use crate::atomic::{Entry, Hash, MessageEncoded, Validation};
use crate::key_pair::KeyPair;
use crate::Result;

/// Custom error types for `EntrySigned`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum EntrySignedError {
    /// Encoded entry string contains invalid hex characters.
    #[error("invalid hex encoding in entry")]
    InvalidHexEncoding,

    /// Can not sign and encode an entry without a `Message`.
    #[error("entry does not contain any message")]
    MessageMissing,
}

/// Bamboo entry bytes represented in hex encoding format.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "db-sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct EntrySigned(String);

impl EntrySigned {
    /// Validates and wraps encoded entry string into a new `EntrySigned` instance.
    pub fn new(value: &str) -> Result<Self> {
        let inner = Self(value.to_owned());
        inner.validate()?;
        Ok(inner)
    }

    /// Returns YAMF BLAKE2b hash of encoded entry.
    pub fn hash(&self) -> Hash {
        Hash::new_from_bytes(self.to_bytes()).unwrap()
    }

    /// Returns encoded entry as string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Decodes hex encoding and returns entry as bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Unwrap as we already know that the inner value is valid
        hex::decode(&self.0).unwrap()
    }

    /// Returns payload size (number of bytes) of total encoded entry.
    pub fn size(&self) -> u64 {
        self.0.len() as u64 / 2
    }
}

/// Converts an `EntrySigned` into a Bamboo Entry to interact with the `bamboo_rs` crate.
impl From<&EntrySigned> for BambooEntry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> {
    fn from(signed_entry: &EntrySigned) -> Self {
        let entry_bytes = signed_entry.clone().to_bytes();
        let entry_ref: BambooEntry<&[u8], &[u8]> = entry_bytes.as_slice().try_into().unwrap();
        bamboo_rs_core::entry::into_owned(&entry_ref)
    }
}

impl TryFrom<&[u8]> for EntrySigned {
    type Error = anyhow::Error;

    fn try_from(bytes: &[u8]) -> std::result::Result<Self, Self::Error> {
        Self::new(&hex::encode(bytes))
    }
}

/// Takes an Entry, signs it with private key and returns signed and encoded version in form of an
/// [`EntrySigned`] instance.
///
/// After conversion the result is ready to be sent to a p2panda node.
impl TryFrom<(&Entry, &KeyPair)> for EntrySigned {
    type Error = anyhow::Error;

    fn try_from((entry, key_pair): (&Entry, &KeyPair)) -> std::result::Result<Self, Self::Error> {
        // Generate message hash
        let message_encoded = match entry.message() {
            Some(message) => MessageEncoded::try_from(message)?,
            None => bail!(EntrySignedError::MessageMissing),
        };
        let message_hash = message_encoded.hash();
        let message_size = message_encoded.size();

        // Convert entry links to bamboo-rs `YamfHash` type
        let backlink = entry.backlink_hash().map(|link| link.to_owned().into());
        let lipmaa_link = entry.skiplink_hash().map(|link| link.to_owned().into());

        // Create bamboo entry. See: https://github.com/AljoschaMeyer/bamboo#encoding for encoding
        // details and definition of entry fields.
        let mut entry: BambooEntry<_, &[u8]> = BambooEntry {
            log_id: entry.log_id().as_u64(),
            is_end_of_feed: false,
            payload_hash: message_hash.into(),
            payload_size: message_size,
            author: PublicKey::from_bytes(&key_pair.public_key_bytes())?,
            seq_num: entry.seq_num().as_u64(),
            backlink,
            lipmaa_link,
            sig: None,
        };

        // Get entry bytes first for signing them with key pair
        let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];
        let unsigned_entry_size = entry.encode(&mut entry_bytes)?;

        // Sign and add signature to entry
        let sig_bytes = key_pair.sign(&entry_bytes[..unsigned_entry_size]);
        let signature = BambooSignature(&*sig_bytes);
        entry.sig = Some(signature);

        // Get entry bytes again, now with signature included
        let signed_entry_size = entry.encode(&mut entry_bytes)?;

        EntrySigned::try_from(&entry_bytes[..signed_entry_size])
    }
}

impl PartialEq for EntrySigned {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Validation for EntrySigned {
    fn validate(&self) -> Result<()> {
        hex::decode(&self.0).map_err(|_| EntrySignedError::InvalidHexEncoding)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use crate::atomic::{Entry, Hash, LogId, Message, MessageEncoded, MessageFields, MessageValue};
    use crate::key_pair::KeyPair;

    use super::EntrySigned;

    #[test]
    fn validate() {
        // Invalid hex string
        assert!(EntrySigned::new("123456789Z").is_err());
    }

    #[test]
    fn sign_and_encode() {
        // Generate Ed25519 key pair to sign entry with
        let key_pair = KeyPair::new();

        // Prepare sample values
        let mut fields = MessageFields::new();
        fields
            .add("test", MessageValue::Text("Hello".to_owned()))
            .unwrap();
        let message =
            Message::new_create(Hash::new_from_bytes(vec![1, 2, 3]).unwrap(), fields).unwrap();

        // Create a p2panda entry, then sign it. For this encoding, the entry is converted into a
        // bamboo-rs-core entry, which means that it also doesn't contain the message anymore.
        let entry = Entry::new(&LogId::default(), &message, None, None, None).unwrap();
        let entry_signed_encoded = EntrySigned::try_from((&entry, &key_pair)).unwrap();

        // Make an unsigned, decoded p2panda entry from the signed and encoded form. This is adding
        // the message back.
        let message_encoded = MessageEncoded::try_from(&message).unwrap();
        let entry_decoded: Entry =
            Entry::try_from((&entry_signed_encoded, Some(&message_encoded))).unwrap();

        // Re-encode the recovered entry to be able to check that we still have the same data.
        let test_entry_signed_encoded = EntrySigned::try_from((&entry_decoded, &key_pair)).unwrap();
        assert_eq!(entry_signed_encoded, test_entry_signed_encoded);
    }
}
