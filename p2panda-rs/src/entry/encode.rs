// SPDX-License-Identifier: AGPL-3.0-or-later

use bamboo_rs_core_ed25519_yasmf::entry::{is_lipmaa_required, MAX_ENTRY_SIZE};
use bamboo_rs_core_ed25519_yasmf::{Entry as BambooEntry, Signature as BambooSignature};

use crate::entry::error::EncodeEntryError;
use crate::entry::validate::validate_links;
use crate::entry::{EncodedEntry, Entry, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::EncodedOperation;

/// Takes entry arguments (log id, sequence number, etc.), operation payload and a [`KeyPair`],
/// returns signed `Entry` instance.
///
/// The result can be converted to an `EncodedEntry` using the `encode_entry` method and is then
/// ready to be sent to a p2panda node.
///
/// Using this method we can assume that the entry will be correctly signed. This applies only
/// basic checks if the backlink and skiplink is correctly set for the given sequence number (#E3).
/// Please note though that this method not check for correct log integrity!
pub fn sign_entry(
    log_id: &LogId,
    seq_num: &SeqNum,
    skiplink_hash: Option<&Hash>,
    backlink_hash: Option<&Hash>,
    payload: &EncodedOperation,
    key_pair: &KeyPair,
) -> Result<Entry, EncodeEntryError> {
    // Generate payload hash and size from operation bytes
    let payload_hash = payload.hash();
    let payload_size = payload.size();

    // Convert entry links to bamboo-rs `YasmfHash` type
    let backlink = backlink_hash.map(|link| link.into());
    let lipmaa_link = if is_lipmaa_required(seq_num.as_u64()) {
        skiplink_hash.map(|link| link.into())
    } else {
        // Omit skiplink when it is the same as backlink, this saves us some bytes
        None
    };

    // Create Bamboo entry instance.
    //
    // See: https://github.com/AljoschaMeyer/bamboo#encoding for encoding details and definition of
    // entry fields.
    let mut entry: BambooEntry<_, &[u8]> = BambooEntry {
        is_end_of_feed: false,
        author: key_pair.public_key().to_owned(),
        log_id: log_id.as_u64(),
        seq_num: seq_num.as_u64(),
        lipmaa_link,
        backlink,
        payload_size,
        payload_hash: (&payload_hash).into(),
        sig: None,
    };

    let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];

    // Get unsigned entry bytes
    let entry_size = entry.encode(&mut entry_bytes)?;

    // Sign entry
    let signature = key_pair.sign(&entry_bytes[..entry_size]);
    let signature_bytes = signature.into_bytes();

    let signed_entry = Entry {
        author: key_pair.public_key().into(),
        log_id: log_id.to_owned(),
        seq_num: seq_num.to_owned(),
        skiplink: skiplink_hash.cloned(),
        backlink: backlink_hash.cloned(),
        payload_size,
        payload_hash,
        signature: signature_bytes[..].into(),
    };

    // Make sure the links are correct (#E3)
    validate_links(&signed_entry)?;

    Ok(signed_entry)
}

/// Encodes an entry into bytes and returns them as `EncodedEntry` instance. After encoding this is
/// ready to be sent to a p2panda node.
///
/// This method only fails if something went wrong with the encoder or if a backlink was provided
/// on an entry with sequence number 1 (#E3).
pub fn encode_entry(entry: &Entry) -> Result<EncodedEntry, EncodeEntryError> {
    let signature_bytes = entry.signature().into_bytes();

    let mut entry: BambooEntry<_, &[u8]> = BambooEntry {
        is_end_of_feed: false,
        author: entry.author().into(),
        log_id: entry.log_id().as_u64(),
        seq_num: entry.seq_num().as_u64(),
        lipmaa_link: entry.skiplink().map(|link| link.into()),
        backlink: entry.backlink().map(|link| link.into()),
        payload_size: entry.payload_size(),
        payload_hash: entry.payload_hash().into(),
        sig: Some(BambooSignature(&signature_bytes[..])),
    };

    let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];

    // Together with signing the entry before, one could think that encoding the entry a second
    // time is a waste, but actually it is the only way to do signatures. This step is not
    // redundant.
    //
    // Calling this also checks if the backlink is not set for the first entry (#E3).
    let signed_entry_size = entry.encode(&mut entry_bytes)?;

    Ok(EncodedEntry::from(&entry_bytes[..signed_entry_size]))
}

/// High-level method which applies both signing and encoding an entry into one step, returns an
/// `EncodedEntry` instance which is ready to be sent to a p2panda node.
///
/// See low-level methods for details.
pub fn sign_and_encode_entry(
    log_id: &LogId,
    seq_num: &SeqNum,
    skiplink_hash: Option<&Hash>,
    backlink_hash: Option<&Hash>,
    payload: &EncodedOperation,
    key_pair: &KeyPair,
) -> Result<EncodedEntry, EncodeEntryError> {
    let entry = sign_entry(
        log_id,
        seq_num,
        skiplink_hash,
        backlink_hash,
        payload,
        key_pair,
    )?;

    let encoded_entry = encode_entry(&entry)?;

    Ok(encoded_entry)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::{Entry, EntryBuilder, LogId, SeqNum};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::{OperationBuilder, OperationValue};
    use crate::schema::Schema;
    use crate::test_utils::fixtures::{key_pair, random_hash, schema};

    use super::{encode_entry, sign_entry};

    #[rstest]
    fn sign(
        #[from(random_hash)] skiplink_hash: Hash,
        #[from(random_hash)] backlink_hash: Hash,
        schema: Schema,
        key_pair: KeyPair,
    ) {
        let operation = OperationBuilder::new(&schema)
            .fields(&[("test", OperationValue::Text("test".to_owned()))])
            .build()
            .unwrap();

        let entry = EntryBuilder::new()
            .seq_num(&SeqNum::new(1).unwrap())
            .log_id(&LogId::new(0))
            .skiplink(&skiplink_hash)
            .backlink(&backlink_hash)
            .operation(&operation)
            .sign(&key_pair)
            .unwrap();
    }
}
