// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use bamboo_rs_core_ed25519_yasmf::entry::MAX_ENTRY_SIZE;
use bamboo_rs_core_ed25519_yasmf::{Entry as BambooEntry, Signature as BambooSignature};

use crate::entry::{Entry, EntrySigned, EntrySignedError};
use crate::identity::KeyPair;
use crate::message::MessageEncoded;

/// Takes an [`Entry`] and a [`KeyPair`], returns signed and encoded entry bytes in form of an
/// [`EntrySigned`] instance.
///
/// After conversion the result is ready to be sent to a p2panda node.
///
/// ## Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use p2panda_rs::entry::{Entry, EntrySigned, LogId, SeqNum, sign_and_encode};
/// use p2panda_rs::message::{Message, MessageFields, MessageValue};
/// use p2panda_rs::hash::Hash;
/// use p2panda_rs::identity::KeyPair;
///
/// // Generate Ed25519 key pair to sign entry with
/// let key_pair = KeyPair::new();
///
/// // Create message
/// let schema_hash = Hash::new("004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8")?;
/// let mut fields = MessageFields::new();
/// fields.add("title", MessageValue::Text("Hello, Panda!".to_owned()))?;
/// let message = Message::new_create(schema_hash, fields)?;
///
/// // Create entry
/// let entry = Entry::new(
///     &LogId::default(),
///     Some(&message),
///     None,
///     None,
///     &SeqNum::new(1)?,
/// )?;
///
/// // Sign and encode entry
/// let entry_signed_encoded = sign_and_encode(&entry, &key_pair)?;
/// # Ok(())
/// # }
/// ```
pub fn sign_and_encode(entry: &Entry, key_pair: &KeyPair) -> Result<EntrySigned, EntrySignedError> {
    // Generate message hash
    let message_encoded = match entry.message() {
        Some(message) => MessageEncoded::try_from(message)?,
        None => return Err(EntrySignedError::MessageMissing),
    };
    let message_hash = message_encoded.hash();
    let message_size = message_encoded.size();

    // Convert entry links to bamboo-rs `YasmfHash` type
    let backlink = entry.backlink_hash().map(|link| link.to_owned().into());
    let lipmaa_link = if entry.is_skiplink_required() {
        if entry.skiplink_hash().is_none() {
            return Err(EntrySignedError::SkiplinkMissing);
        }
        entry.skiplink_hash().map(|link| link.to_owned().into())
    } else {
        // Omit skiplink when it is the same as backlink, this saves us some bytes
        None
    };

    // Create bamboo entry. See: https://github.com/AljoschaMeyer/bamboo#encoding for encoding
    // details and definition of entry fields.
    let mut entry: BambooEntry<_, &[u8]> = BambooEntry {
        log_id: entry.log_id().as_i64() as u64,
        is_end_of_feed: false,
        payload_hash: message_hash.into(),
        payload_size: message_size as u64,
        author: key_pair.public_key().to_owned(),
        seq_num: entry.seq_num().as_i64() as u64,
        backlink,
        lipmaa_link,
        sig: None,
    };

    let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];

    // Get unsigned entry bytes
    let entry_size = entry.encode(&mut entry_bytes)?;

    // Sign and add signature to entry
    let signature = key_pair.sign(&entry_bytes[..entry_size]);
    let signature_bytes = signature.to_bytes();
    entry.sig = Some(BambooSignature(&signature_bytes[..]));

    // Get signed entry bytes
    let signed_entry_size = entry.encode(&mut entry_bytes)?;

    // Return signed entry bytes in the form of an EntrySigned
    EntrySigned::try_from(&entry_bytes[..signed_entry_size])
}
