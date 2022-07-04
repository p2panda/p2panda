// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use bamboo_rs_core_ed25519_yasmf::entry::MAX_ENTRY_SIZE;
use bamboo_rs_core_ed25519_yasmf::{Signature as BambooSignature, YasmfHash};
use lipmaa_link::is_skip_link;
use rstest::fixture;
use varu64::encode as varu64_encode;

use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::{Blake3ArrayVec, Hash};
use crate::identity::KeyPair;
use crate::operation::{Operation, OperationEncoded};

use crate::test_utils::constants::default_fields;
use crate::test_utils::fixtures::{key_pair, operation, operation_fields, random_hash};

/// Fixture which injects an `Entry` into a test method. Default values are those of
/// the first entry in log number 1. The default payload is a CREATE operation containing
/// the default testing fields.
///
/// Default values can be overridden at testing time by passing in custom operation, seq number, log_id,
/// backlink, skiplink and operation. The `#[with()]` tag can be used to partially change default
/// values.
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # #[cfg(test)]
/// # mod tests {
/// use rstest::rstest;
/// use p2panda_rs::test_utils::fixtures::entry;
///
/// #[rstest]
/// fn inserts_the_default_entry(entry: Entry) {
///     assert_eq!(entry.seq_num().as_u64(), 1)
/// }
///
/// #[rstest]
/// fn just_change_the_log_id(#[with(1, 2)] entry: Entry) {
///     assert_eq!(entry.seq_num().as_u64(), 2)
/// }
///
/// #[rstest]
/// #[case(entry(1, 1, None, None, None))]
/// #[should_panic]
/// #[case(entry(0, 1, None, None, None))]
/// #[should_panic]
/// #[case::panic(entry(1, 1, Some(DEFAULT_HASH.parse().unwrap()), None, None))]
/// fn different_cases_pass_or_panic(#[case] _entry: Entry) {}
///
/// # }
/// # Ok(())
/// # }
/// ```
#[fixture]
pub fn entry(
    #[default(1)] seq_num: u64,
    #[default(1)] log_id: u64,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
    #[default(Some(operation(Some(operation_fields(default_fields())), None, None)))]
    operation: Option<Operation>,
) -> Entry {
    Entry::new(
        &LogId::new(log_id),
        operation.as_ref(),
        skiplink.as_ref(),
        backlink.as_ref(),
        &SeqNum::new(seq_num).unwrap(),
    )
    .unwrap()
}

/// Fixture which injects an `Entry` with auto generated valid values for backlink, skiplink and
/// operation.
///
/// seq_num and log_id can be overridden at testing time by passing in custom values. The
/// `#[with()]` tag can be used to partially change default values.
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # #[cfg(test)]
/// # mod tests {
/// use rstest::rstest;
/// use p2panda_rs::test_utils::fixtures::entry_auto_gen_links;
///
/// #[rstest]
/// fn just_change_the_seq_num(
///     #[from(entry_auto_gen_links)]
///     #[with(30)] // This seq_num should have a backlink and skiplink
///     entry: Entry,
/// ) {
///     assert_eq!(entry.seq_num().as_u64(), 30);
///     assert_eq!(entry.log_id().as_u64(), 1);
///     assert!(entry.backlink_hash().is_some());
///     assert!(entry.skiplink_hash().is_some())
/// }
///
/// // The fixtures can also be used as a constructor within the test itself.
/// //
/// // Here we combine that functionality with another `rstest` feature `#[value]`. This test runs once for
/// // every combination of values provided.
/// #[rstest]
/// fn used_as_constructor(#[values(1, 2, 3, 4)] seq_num: u64, #[values(1, 2, 3, 4)] log_id: u64) {
///     let entry = entry_auto_gen_links(seq_num, log_id);
///
///     assert_eq!(entry.seq_num().as_u64(), seq_num);
///     assert_eq!(entry.log_id().as_u64(), log_id)
/// }
/// # }
/// # Ok(())
/// # }
/// ```
#[fixture]
pub fn entry_auto_gen_links(#[default(1)] seq_num: u64, #[default(1)] log_id: u64) -> Entry {
    let backlink = match seq_num {
        1 => None,
        _ => Some(random_hash()),
    };

    let skiplink = match is_skip_link(seq_num) {
        false => None,
        true => Some(random_hash()),
    };

    Entry::new(
        &LogId::new(log_id),
        Some(&operation(
            Some(operation_fields(default_fields())),
            backlink.clone().map(|hash| hash.into()),
            None,
        )),
        skiplink.as_ref(),
        backlink.as_ref(),
        &SeqNum::new(seq_num).unwrap(),
    )
    .unwrap()
}

/// Fixture which injects the default testing EntrySigned into a test method.
///
/// Default values can be overridden at testing time by passing in custom entry and
/// key pair.
#[fixture]
pub fn entry_signed_encoded(entry: Entry, key_pair: KeyPair) -> EntrySigned {
    sign_and_encode(&entry, &key_pair).unwrap()
}

/// Fixture which injects the default testing EntrySigned into a test method WITHOUT any validation
/// during construction.
///
/// Default values can be overridden at testing time by passing in custom entry and
/// key pair.
#[fixture]
pub fn entry_signed_encoded_unvalidated(
    #[default(1)] seq_num: u64,
    #[default(1)] log_id: u64,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
    #[default(Some(operation(Some(operation_fields(default_fields())), None, None)))]
    operation: Option<Operation>,
    key_pair: KeyPair,
) -> String {
    let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];

    let mut next_byte_num = 0;

    // Encode the end of feed.
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
            next_byte_num += Into::<YasmfHash<Blake3ArrayVec>>::into(lipmaa_link)
                .encode(&mut entry_bytes[next_byte_num..])
                .unwrap();
            next_byte_num
        }
        _ => next_byte_num,
    };

    // Encode the backlink link
    next_byte_num = match backlink {
        Some(backlink) => {
            next_byte_num += Into::<YasmfHash<Blake3ArrayVec>>::into(backlink)
                .encode(&mut entry_bytes[next_byte_num..])
                .unwrap();
            next_byte_num
        }
        _ => next_byte_num,
    };

    // Encode the operation if it exists.
    match operation {
        Some(operation) => {
            let operation_encoded = OperationEncoded::try_from(&operation).unwrap();
            // Encode the payload size
            let operation_size = operation_encoded.size();
            next_byte_num += varu64_encode(operation_size, &mut entry_bytes[next_byte_num..]);

            // Encode the payload hash
            let operation_hash = operation_encoded.hash();
            next_byte_num += Into::<YasmfHash<Blake3ArrayVec>>::into(operation_hash)
                .encode(&mut entry_bytes[next_byte_num..])
                .unwrap();
        }
        None => (),
    };

    // Attach signature.
    let signature = key_pair.sign(&entry_bytes[..next_byte_num]);
    let signature_bytes = signature.to_bytes();
    let sig = Some(BambooSignature(&signature_bytes[..])).unwrap();

    // Trim bytes.
    next_byte_num += sig.encode(&mut entry_bytes[next_byte_num..]).unwrap();

    // Return hex encoded entry bytes.
    hex::encode(&entry_bytes[..next_byte_num])
}
