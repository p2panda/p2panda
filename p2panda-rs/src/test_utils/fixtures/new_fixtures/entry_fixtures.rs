use rstest::fixture;

use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::Operation;

use crate::test_utils::fixtures::*;

#[fixture]
pub fn log_id(#[default(1)] id: u64) -> LogId {
    LogId::new(id)
}

/// Fixture which injects the default Entry into a test method.
///
/// Default value can be overridden at testing time by passing in custom operation, seq number,
/// backlink, skiplink and log_id.
#[fixture]
pub fn entry(
    operation: Operation,
    seq_num: SeqNum,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
    #[default(LogId::default())] log_id: LogId,
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

#[fixture]
pub fn entry_signed_encoded(entry: Entry, key_pair: KeyPair) -> EntrySigned {
    sign_and_encode(&entry, &key_pair).unwrap()
}
