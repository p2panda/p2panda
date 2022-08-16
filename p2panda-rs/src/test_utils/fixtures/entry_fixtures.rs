// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;

use bamboo_rs_core_ed25519_yasmf::entry::MAX_ENTRY_SIZE;
use bamboo_rs_core_ed25519_yasmf::{Signature as BambooSignature, YasmfHash};
use lipmaa_link::is_skip_link;
use rstest::fixture;
use varu64::encode as varu64_encode;

use crate::entry::encode::{encode_entry, sign_entry};
use crate::entry::{EncodedEntry, Entry};
use crate::hash::{Blake3ArrayVec, Hash};
use crate::identity::KeyPair;
use crate::operation::encode::encode_operation;
use crate::operation::{EncodedOperation, Operation};
use crate::test_utils::fixtures::{encoded_operation, key_pair, operation, random_hash};

/// Creates an `Entry`.
///
/// Default values are those of the first entry in log number 1. The default payload is a CREATE
/// operation containing the default testing fields. All values can be overridden at testing time
/// by passing in custom operation, seq number, log_id, backlink, skiplink and operation. The
/// `#[with()]` tag can be used to partially change default values.
#[fixture]
pub fn entry(
    #[default(1)] seq_num: u64,
    #[default(0)] log_id: u64,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
    #[from(encoded_operation)] encoded_operation: EncodedOperation,
    #[from(key_pair)] key_pair: KeyPair,
) -> Entry {
    sign_entry(
        &log_id.into(),
        &seq_num.try_into().unwrap(),
        skiplink.as_ref(),
        backlink.as_ref(),
        &encoded_operation,
        &key_pair,
    )
    .unwrap()
}

/// Creates an `Entry` with auto generated valid values for backlink, skiplink and operation.
///
/// `seq_num` and `log_id` can be overridden at testing time by passing in custom values. The
/// `#[with()]` tag can be used to partially change default values.
#[fixture]
pub fn entry_auto_gen_links(
    #[default(1)] seq_num: u64,
    #[default(0)] log_id: u64,
    #[from(encoded_operation)] encoded_operation: EncodedOperation,
    #[from(key_pair)] key_pair: KeyPair,
) -> Entry {
    let backlink = match seq_num {
        1 => None,
        _ => Some(random_hash()),
    };

    let skiplink = match is_skip_link(seq_num) {
        false => None,
        true => Some(random_hash()),
    };

    entry(
        seq_num,
        log_id,
        backlink,
        skiplink,
        encoded_operation,
        key_pair,
    )
}

/// Returns default encoded entry.
#[fixture]
pub fn encoded_entry(
    #[default(1)] seq_num: u64,
    #[default(0)] log_id: u64,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
    #[from(encoded_operation)] encoded_operation: EncodedOperation,
    #[from(key_pair)] key_pair: KeyPair,
) -> EncodedEntry {
    let entry = sign_entry(
        &log_id.into(),
        &seq_num.try_into().unwrap(),
        skiplink.as_ref(),
        backlink.as_ref(),
        &encoded_operation,
        &key_pair,
    )
    .unwrap();

    encode_entry(&entry).unwrap()
}

/// Creates encoded entry which was created WITHOUT any validation.
///
/// Default values can be overridden at testing time by passing in custom entry and key pair.
#[fixture]
pub fn entry_signed_encoded_unvalidated(
    #[default(1)] seq_num: u64,
    #[default(0)] log_id: u64,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
    #[default(None)] operation: Option<EncodedOperation>,
    #[from(key_pair)] key_pair: KeyPair,
) -> EncodedEntry {
    let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];
    let mut next_byte_num = 0;

    // Encode the end of feed
    entry_bytes[0] = 0;
    next_byte_num += 1;

    // Encode the author
    let author_bytes = key_pair.public_key().as_bytes();
    entry_bytes[next_byte_num..author_bytes.len() + next_byte_num]
        .copy_from_slice(&author_bytes[..]);
    next_byte_num += author_bytes.len();

    // Encode the log_id
    next_byte_num += varu64_encode(log_id, &mut entry_bytes[next_byte_num..]);

    // Encode the sequence number
    next_byte_num += varu64_encode(seq_num, &mut entry_bytes[next_byte_num..]);

    // Encode the lipmaa link
    next_byte_num = match skiplink {
        Some(lipmaa_link) => {
            next_byte_num += Into::<YasmfHash<Blake3ArrayVec>>::into(&lipmaa_link)
                .encode(&mut entry_bytes[next_byte_num..])
                .unwrap();
            next_byte_num
        }
        _ => next_byte_num,
    };

    // Encode the backlink link
    next_byte_num = match backlink {
        Some(backlink) => {
            next_byte_num += Into::<YasmfHash<Blake3ArrayVec>>::into(&backlink)
                .encode(&mut entry_bytes[next_byte_num..])
                .unwrap();
            next_byte_num
        }
        _ => next_byte_num,
    };

    // Encode the operation if it exists
    match operation {
        Some(operation) => {
            // Encode the payload size
            let operation_size = operation.size();
            next_byte_num += varu64_encode(operation_size, &mut entry_bytes[next_byte_num..]);

            // Encode the payload hash
            let operation_hash = operation.hash();
            next_byte_num += Into::<YasmfHash<Blake3ArrayVec>>::into(&operation_hash)
                .encode(&mut entry_bytes[next_byte_num..])
                .unwrap();
        }
        None => (),
    };

    // Attach signature
    let signature = key_pair.sign(&entry_bytes[..next_byte_num]);
    let signature_bytes = signature.to_bytes();
    let sig = Some(BambooSignature(&signature_bytes[..])).unwrap();

    // Trim bytes
    next_byte_num += sig.encode(&mut entry_bytes[next_byte_num..]).unwrap();

    EncodedEntry::from_bytes(&entry_bytes[..next_byte_num])
}
