// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;

use bamboo_rs_core_ed25519_yasmf::entry::MAX_ENTRY_SIZE;
use bamboo_rs_core_ed25519_yasmf::{Signature as BambooSignature, YasmfHash};
use lipmaa_link::is_skip_link;
use rstest::fixture;
use varu64::encode as varu64_encode;

use crate::next::entry::encode::sign_entry;
use crate::next::entry::Entry;
use crate::next::hash::{Blake3ArrayVec, Hash};
use crate::next::identity::KeyPair;
use crate::next::operation::encode::encode_operation;
use crate::next::operation::Operation;
use crate::next::test_utils::fixtures::{key_pair, operation, random_hash};

/// Fixture which injects an `Entry` into a test method.
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
    #[from(operation)] operation: Operation,
    #[from(key_pair)] key_pair: KeyPair,
) -> Entry {
    let encoded_operation = encode_operation(&operation).unwrap();

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

/// Fixture which injects an `Entry` with auto generated valid values for backlink, skiplink and
/// operation.
///
/// seq_num and log_id can be overridden at testing time by passing in custom values. The
/// `#[with()]` tag can be used to partially change default values.
/// ```
#[fixture]
pub fn entry_auto_gen_links(
    #[default(1)] seq_num: u64,
    #[default(0)] log_id: u64,
    #[from(operation)] operation: Operation,
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

    entry(seq_num, log_id, backlink, skiplink, operation, key_pair)
}

/// Fixture which injects the default testing EntrySigned into a test method WITHOUT any validation
/// during construction.
///
/// Default values can be overridden at testing time by passing in custom entry and key pair.
#[fixture]
pub fn entry_signed_encoded_unvalidated(
    #[default(1)] seq_num: u64,
    #[default(0)] log_id: u64,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
    #[from(operation)] operation: Operation,
    #[from(key_pair)] key_pair: KeyPair,
) -> String {
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

    // Encode the operation
    let operation_encoded = encode_operation(&operation).unwrap();

    // Encode the payload size
    let operation_size = operation_encoded.size();
    next_byte_num += varu64_encode(operation_size, &mut entry_bytes[next_byte_num..]);

    // Encode the payload hash
    let operation_hash = operation_encoded.hash();
    next_byte_num += Into::<YasmfHash<Blake3ArrayVec>>::into(&operation_hash)
        .encode(&mut entry_bytes[next_byte_num..])
        .unwrap();

    // Attach signature
    let signature = key_pair.sign(&entry_bytes[..next_byte_num]);
    let signature_bytes = signature.to_bytes();
    let sig = Some(BambooSignature(&signature_bytes[..])).unwrap();

    // Trim bytes
    next_byte_num += sig.encode(&mut entry_bytes[next_byte_num..]).unwrap();

    // Return hex encoded entry bytes
    hex::encode(&entry_bytes[..next_byte_num])
}
