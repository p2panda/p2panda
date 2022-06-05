// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::Operation;

use crate::test_utils::fixtures::{key_pair, operation};

/// Fixture which injects the default testing SeqNum(1) into a test method.
///
/// Default value can be overridden at testing time by passing in a custom seq num as u64.
#[fixture]
pub fn seq_num(#[default(1)] n: u64) -> SeqNum {
    SeqNum::new(n).unwrap()
}

/// Fixture which injects the default testing LogId(1) into a test method.
///
/// Default value can be overridden at testing time by passing in a custom log id as u64.
#[fixture]
pub fn log_id(#[default(1)] id: u64) -> LogId {
    LogId::new(id)
}

/// Fixture which injects the default testing Entry into a test method.
///
/// Default value can be overridden at testing time by passing in custom operation, seq number,
/// backlink, skiplink and log_id.
#[fixture]
pub fn entry(
    operation: Operation,
    seq_num: SeqNum,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
    log_id: LogId,
) -> Entry {
    Entry::new(
        &log_id,
        Some(&operation),
        skiplink.as_ref(),
        backlink.as_ref(),
        &seq_num,
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
