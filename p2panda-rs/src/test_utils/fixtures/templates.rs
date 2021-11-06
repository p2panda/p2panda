// SPDX-License-Identifier: AGPL-3.0-or-later

//! With these templates you can apply many rstest cases to a single test. They utilize the somewhat experimental
//! [rstest_reuse](https://github.com/la10736/rstest/tree/master/rstest_reuse) crate.
use rstest_reuse::template;

// This template contains several different messages which should fail when run against the default
// fixture message.
#[template]
#[rstest]
// This flag states that the tested case should panic
#[should_panic]
#[case::wrong_message(crate::test_utils::fixtures::create_message(MESSAGE_SCHEMA.into(), fields(vec![("message", "Boo!")])))]
#[should_panic]
#[case::wrong_message(crate::test_utils::fixtures::create_message(MESSAGE_SCHEMA.into(), fields(vec![("date", "2021-05-02T20:06:45.430Z")])))]
#[should_panic]
#[case::wrong_message(crate::test_utils::fixtures::create_message(MESSAGE_SCHEMA.into(), fields(vec![("message", "Hello!"), ("date", "2021-05-02T20:06:45.430Z")])))]
fn messages_not_matching_entry_should_fail(
    entry: Entry,
    #[case] message: Message,
    key_pair: KeyPair,
) {
}

// This template contains various types of valid entry.
#[template]
#[rstest]
#[case::first_entry(crate::test_utils::fixtures::defaults::first_entry())]
#[case::entry_with_backlink(crate::test_utils::fixtures::defaults::entry_with_backlink())]
#[case::entry_with_backlink_and_skiplink(crate::test_utils::fixtures::defaults::entry_with_backlink_and_skiplink())]
fn many_entry_versions(#[case] entry: Entry, key_pair: KeyPair) {}

// This template contains various types of valid message.
#[template]
#[rstest]
#[case::create_message(crate::test_utils::fixtures::defaults::create_message())]
#[case::update_message(crate::test_utils::fixtures::defaults::update_message())]
#[case::delete_message(crate::test_utils::fixtures::defaults::delete_message())]
fn all_message_types(entry: Entry, #[case] message: Message, key_pair: KeyPair) {}

// Template which will contain many version fixtures in the future.
#[template]
#[rstest]
#[case::v0_1_0(crate::test_utils::fixtures::v0_1_0_fixture())]
fn version_fixtures(#[case] fixture: Fixture) {}

// Here we export the macros for use in the rest of the crate.
#[allow(unused_imports)]
pub(crate) use all_message_types;
#[allow(unused_imports)]
pub(crate) use many_entry_versions;
#[allow(unused_imports)]
pub(crate) use messages_not_matching_entry_should_fail;
#[allow(unused_imports)]
pub(crate) use version_fixtures;
