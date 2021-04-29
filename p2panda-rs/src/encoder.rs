use std::convert::{TryFrom, TryInto};

use bamboo_rs_core::entry::{decode, MAX_ENTRY_SIZE};
use bamboo_rs_core::{Entry as BambooEntry, Signature as BambooSignature, YamfHash};

use crate::atomic::{Entry, EntrySigned, Hash, LogId, Message, MessageEncoded, SeqNum};
use crate::atomic::error::EntrySignedError;
use crate::key_pair::KeyPair;
use crate::atomic::Blake2BArrayVec;
use arrayvec::ArrayVec;
use ed25519_dalek::PublicKey;


/// Takes an [`EntrySigned`] and a [`MessageEncoded`], validates the encoded message against the entry payload hash, 
/// returns the decoded message in the form of a [`Message`] if valid otherwise throws an error.
pub fn validate_message(entry_encoded: &EntrySigned, message_encoded: &MessageEncoded) -> Result<Message, EntrySignedError> {
    // Convert to Entry from bamboo_rs_core first
    let entry: BambooEntry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> = entry_encoded.into();
    // Messages may be omitted because the entry still contains the message hash. If the
    // message is explicitly included we require its hash to match.
    let message = match message_encoded {
        msg => {
            let yamf_hash: YamfHash<Blake2BArrayVec> =
                (&msg.hash()).to_owned().into();

            if yamf_hash != entry.payload_hash {
                return Err(EntrySignedError::MessageHashMismatch);
            }
            Message::from(msg)
        }
    };
    Ok(message)
}

/// Takes an [`Entry`] and a public key, returns a tuple containing encoded entry bytes and their size.
pub fn encode_entry(entry: &Entry, public_key: &Box<[u8]>) -> Result<(usize, [u8; MAX_ENTRY_SIZE]), EntrySignedError> {
    // Generate message hash
    let message_encoded = match entry.message() {
        Some(message) => MessageEncoded::try_from(message)?,
        None => return Err(EntrySignedError::MessageMissing),
    };
    let message_hash = message_encoded.hash();
    let message_size = message_encoded.size();

    // Convert entry links to bamboo-rs `YamfHash` type
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
    let entry: BambooEntry<_, &[u8]> = BambooEntry {
        log_id: entry.log_id().as_i64() as u64,
        is_end_of_feed: false,
        payload_hash: message_hash.into(),
        payload_size: message_size as u64,
        author: PublicKey::from_bytes(public_key)?,
        seq_num: entry.seq_num().as_i64() as u64,
        backlink,
        lipmaa_link,
        sig: None,
    };

    let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];
    
    // Get unsigned entry bytes
    let entry_size = entry.encode(&mut entry_bytes)?;
    Ok((entry_size, entry_bytes))
}

/// Takes unsigned entry bytes and their size and a [`KeyPair`], returns a tuple containing signed and encoded entry bytes and their size.
pub fn sign_entry(entry_bytes: [u8; MAX_ENTRY_SIZE], unsigned_entry_size: usize, key_pair: &KeyPair) -> Result<(usize, [u8; MAX_ENTRY_SIZE]), EntrySignedError>{
    // Make copy of entry_bytes before passing to decode
    let mut entry_bytes_copy = entry_bytes.clone();
    
    // Decode unsigned entry bytes
    let mut entry = decode(&entry_bytes)?;
    
    // Sign and add signature to entry
    let sig_bytes = key_pair.sign(&entry_bytes_copy[..unsigned_entry_size]);
    let signature = BambooSignature(&*sig_bytes);
    entry.sig = Some(signature);

    // Get signed entry bytes
    let signed_entry_size = entry.encode(&mut entry_bytes_copy)?;
    Ok((signed_entry_size, entry_bytes_copy))
}

/// Takes an [`Entry`] and a [`KeyPair`], returns signed and encoded entry bytes in form of an
/// [`EntrySigned`] instance.
///
/// After signing the result is ready to be sent to a p2panda node.
pub fn sign_and_encode(entry: &Entry, key_pair: &KeyPair) -> Result<EntrySigned, EntrySignedError> {

    // Get unsigned entry bytes
    let (unsigned_entry_size, unsigned_entry_bytes) = encode_entry(entry, &key_pair.public_key_bytes())?;
    
    // Sign entry and get signed entry bytes
    let (signed_entry_size, signed_entry_bytes) = sign_entry(unsigned_entry_bytes, unsigned_entry_size, key_pair)?;
    
    // Return signed entry bytes in the form of an EntrySigned
    EntrySigned::try_from(&signed_entry_bytes[..signed_entry_size])
}

/// Takes [`EntrySigned`] and optionally [`MessageEncoded`] as arguments, returns a decoded and unsigned [`Entry`]. When a [`MessageEncoded`] is passed
/// it will automatically check its integrity with this [`Entry`] by comparing their hashes. Valid messages will be included in the returned 
/// [`Entry`], if an invalid message is passed an error will be returned.
/// 
/// Entries are separated from the messages they refer to. Since messages can independently be
/// deleted they can be passed on as an optional argument. When a [`Message`] is passed
/// it will automatically check its integrity with this Entry by comparing their hashes.
pub fn decode_entry(entry_encoded: &EntrySigned, message_encoded: Option<&MessageEncoded>) -> Result<Entry, EntrySignedError> {
    // Convert to Entry from bamboo_rs_core first
    let entry: BambooEntry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> = entry_encoded.into();

    let message = match message_encoded {
        Some(msg) => Some(validate_message(entry_encoded, msg)?),
        None => None,
    };

    let entry_hash_backlink: Option<Hash> = match entry.backlink {
        Some(link) => Some(link.try_into()?),
        None => None,
    };

    let entry_hash_skiplink: Option<Hash> = match entry.lipmaa_link {
        Some(link) => Some(link.try_into()?),
        None => None,
    };

    Ok(Entry::new(
        &LogId::new(entry.log_id as i64),
        message.as_ref(),
        entry_hash_skiplink.as_ref(),
        entry_hash_backlink.as_ref(),
        &SeqNum::new(entry.seq_num as i64).unwrap(),
    ).unwrap())
}
