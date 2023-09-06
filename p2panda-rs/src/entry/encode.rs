// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods to sign and encode an entry.
//!
//! Create a new `Entry` instance using the `EntryBuilder` or the alternative low-level
//! `sign_entry` method which takes in the entry arguments and `KeyPair` for signing. Use
//! `encode_entry` to create an `EncodedEntry` instance which can then be serialised and sent to a
//! p2panda node.
//!
//! ```text
//! ┌─────┐                     ┌────────────┐
//! │Entry│ ──encode_entry()──► │EncodedEntry│ ─────► bytes
//! └─────┘                     └────────────┘
//! ```
use bamboo_rs_core_ed25519_yasmf::entry::{is_lipmaa_required, MAX_ENTRY_SIZE};
use bamboo_rs_core_ed25519_yasmf::{Entry as BambooEntry, Signature as BambooSignature};

use crate::entry::error::EncodeEntryError;
use crate::entry::traits::AsEntry;
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
    let entry: BambooEntry<_, &[u8]> = BambooEntry {
        is_end_of_feed: false,
        author: key_pair.public_key().into(),
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

    let signed_entry = Entry {
        public_key: key_pair.public_key(),
        log_id: log_id.to_owned(),
        seq_num: seq_num.to_owned(),
        skiplink: skiplink_hash.cloned(),
        backlink: backlink_hash.cloned(),
        payload_size,
        payload_hash,
        signature: signature.into(),
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

    let entry: BambooEntry<_, &[u8]> = BambooEntry {
        is_end_of_feed: false,
        author: entry.public_key().into(),
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

    Ok(EncodedEntry::from_bytes(&entry_bytes[..signed_entry_size]))
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
    use std::collections::HashMap;
    use std::convert::TryInto;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::entry::traits::AsEncodedEntry;
    use crate::entry::{EncodedEntry, Entry, LogId, SeqNum};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::EncodedOperation;
    use crate::test_utils::fixtures::{
        encoded_entry, encoded_operation, entry, key_pair, random_hash, Fixture,
    };
    use crate::test_utils::templates::{many_valid_entries, version_fixtures};

    use super::{encode_entry, sign_and_encode_entry, sign_entry};

    #[rstest]
    #[case(1, false, false)]
    #[case(2, true, false)]
    #[case(3, true, false)]
    #[case(4, true, true)]
    #[case(5, true, false)]
    #[case(6, true, false)]
    #[case(7, true, false)]
    #[case(8, true, true)]
    #[case(9, true, false)]
    #[should_panic]
    #[case::backlink_missing(2, false, false)]
    #[should_panic]
    #[case::skiplink_missing(4, true, false)]
    fn signing_entry_validation(
        #[case] seq_num: u64,
        #[case] backlink: bool,
        #[case] skiplink: bool,
        #[from(random_hash)] entry_hash_1: Hash,
        #[from(random_hash)] entry_hash_2: Hash,
        #[from(encoded_operation)] operation: EncodedOperation,
        #[from(key_pair)] key_pair: KeyPair,
    ) {
        sign_entry(
            &LogId::default(),
            &seq_num.try_into().unwrap(),
            skiplink.then_some(&entry_hash_1),
            backlink.then_some(&entry_hash_2),
            &operation,
            &key_pair,
        )
        .unwrap();

        sign_and_encode_entry(
            &LogId::default(),
            &seq_num.try_into().unwrap(),
            skiplink.then_some(&entry_hash_1),
            backlink.then_some(&entry_hash_2),
            &operation,
            &key_pair,
        )
        .unwrap();
    }

    #[rstest]
    fn encode_entry_to_hex(#[from(entry)] entry: Entry) {
        assert_eq!(
            encode_entry(&entry).unwrap().to_string(),
            concat!(
                "002f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc",
                "960001f902a000205431af655bb810c63d2d9f1ef83be9da5813096186d32c4b",
                "5198a3e8d80a551ddd932393944661be25ea27088a172401ffb7ccd2af191536",
                "a7f9a1b2f5b2dac0848a337550907b2fe2775c1b9c2e112a9e979f7c6730b48b",
                "83e0ebc04d67c907"
            )
        )
    }

    #[rstest]
    fn invalid_sign_entry_links(
        #[from(random_hash)] entry_hash: Hash,
        #[from(encoded_operation)] operation: EncodedOperation,
        #[from(key_pair)] key_pair: KeyPair,
    ) {
        assert_eq!(
            sign_entry(
                &LogId::new(9),
                &SeqNum::new(4).unwrap(),
                Some(&entry_hash),
                None,
                &operation,
                &key_pair
            )
            .unwrap_err()
            .to_string(),
            "backlink and skiplink not valid for this sequence number"
        );

        assert_eq!(
            sign_and_encode_entry(
                &LogId::new(9),
                &SeqNum::new(4).unwrap(),
                Some(&entry_hash),
                None,
                &operation,
                &key_pair
            )
            .unwrap_err()
            .to_string(),
            "backlink and skiplink not valid for this sequence number"
        );
    }

    #[rstest]
    fn it_hashes(encoded_entry: EncodedEntry) {
        // Use encoded entry as key in hash map
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&encoded_entry, key_value.clone());

        // Check if we can retreive it again with that key
        let key_value_retrieved = hash_map.get(&encoded_entry).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }

    #[apply(version_fixtures)]
    fn fixture_encode(#[case] fixture: Fixture) {
        // Encode fixture
        let entry_encoded = encode_entry(&fixture.entry).unwrap();

        // Fixture hash should equal newly encoded entry hash
        assert_eq!(fixture.entry_encoded.hash(), entry_encoded.hash(),);
    }

    #[apply(many_valid_entries)]
    fn fixture_encode_valid_entries(#[case] entry: Entry) {
        assert!(encode_entry(&entry).is_ok());
    }
}
