// SPDX-License-Identifier: AGPL-3.0-or-later

use lipmaa_link::is_skip_link;
use rstest::fixture;

use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::Operation;

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

mod tests {
    // DOC TESTS //

    use crate::entry::Entry;
    use crate::test_utils::constants::DEFAULT_HASH;
    use crate::test_utils::fixtures::{entry, entry_auto_gen_links};

    use rstest::rstest;

    #[rstest]
    fn inserts_the_default_entry(entry: Entry) {
        assert_eq!(entry.seq_num().as_u64(), 1)
    }

    #[rstest]
    fn just_change_the_log_id(#[with(1, 2)] entry: Entry) {
        assert_eq!(entry.log_id().as_u64(), 2)
    }

    #[rstest]
    #[case(entry(1, 1, None, None, None))]
    #[should_panic]
    #[case(entry(0, 1, None, None, None))]
    #[should_panic]
    #[case::panic(entry(1, 1, Some(DEFAULT_HASH.parse().unwrap()), None, None))]
    fn different_cases_pass_or_panic(#[case] _entry: Entry) {}

    #[rstest]
    fn just_change_the_seq_num(
        #[from(entry_auto_gen_links)]
        #[with(30)] // This seq_num should have a backlink and skiplink
        entry: Entry,
    ) {
        assert_eq!(entry.seq_num().as_u64(), 30);
        assert_eq!(entry.log_id().as_u64(), 1);
        assert!(entry.backlink_hash().is_some());
        assert!(entry.skiplink_hash().is_some())
    }

    // The fixtures can also be used as a constructor within the test itself.
    //
    // Here we combine that functionality with another `rstest` feature `#[value]`. This test runs once for
    // every combination of values provided.
    #[rstest]
    fn used_as_constructor(#[values(1, 2, 3, 4)] seq_num: u64, #[values(1, 2, 3, 4)] log_id: u64) {
        let entry = entry_auto_gen_links(seq_num, log_id);

        assert_eq!(entry.seq_num().as_u64(), seq_num);
        assert_eq!(entry.log_id().as_u64(), log_id)
    }
}
