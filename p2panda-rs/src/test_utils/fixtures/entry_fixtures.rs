// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::Operation;

use crate::test_utils::constants::default_fields;
use crate::test_utils::fixtures::{key_pair, operation, operation_fields};

/// Fixture which injects the default testing Entry into a test method.
///
/// Default value can be overridden at testing time by passing in custom operation, seq number,
/// backlink, skiplink and log_id.
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

/// Fixture which injects the default testing EntrySigned into a test method.
///
/// Default values can be overridden at testing time by passing in custom entry and
/// key pair.
#[fixture]
pub fn entry_signed_encoded(entry: Entry, key_pair: KeyPair) -> EntrySigned {
    sign_and_encode(&entry, &key_pair).unwrap()
}
