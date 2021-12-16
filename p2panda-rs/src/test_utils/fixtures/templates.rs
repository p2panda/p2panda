// SPDX-License-Identifier: AGPL-3.0-or-later

//! With these templates you can apply many rstest cases to a single test. They utilise the somewhat experimental
//! [rstest_reuse](https://github.com/la10736/rstest/tree/master/rstest_reuse) crate.

use rstest_reuse::template;
// This template contains several different operations which don't match the default `Entry` fixture
#[template]
#[rstest]
#[allow(unused_qualifications)]
#[should_panic]
#[case::wrong_operation(
    crate::test_utils::fixtures::create_operation(hash(DEFAULT_SCHEMA_HASH),
    crate::test_utils::operation_fields(vec![("message", "Boo!")])))
]
#[allow(unused_qualifications)]
#[should_panic]
#[case::wrong_operation(
    crate::test_utils::fixtures::create_operation(hash(DEFAULT_SCHEMA_HASH),
    crate::test_utils::operation_fields(vec![("date", "2021-05-02T20:06:45.430Z")])))
]
#[allow(unused_qualifications)]
#[should_panic]
#[case::wrong_operation(
    crate::test_utils::fixtures::create_operation(hash(DEFAULT_SCHEMA_HASH),
    crate::test_utils::operation_fields(vec![("message", "Hello!"), ("date", "2021-05-02T20:06:45.430Z")])))
]
fn non_default_operation_values_panic(
    entry: Entry,
    #[case] operation: Operation,
    key_pair: KeyPair,
) {
}

// This template contains various types of valid entries.
#[template]
#[rstest]
#[allow(unused_qualifications)]
#[case::first_entry(crate::test_utils::fixtures::defaults::first_entry())]
#[allow(unused_qualifications)]
#[case::entry_with_backlink(crate::test_utils::fixtures::defaults::entry_with_backlink())]
#[allow(unused_qualifications)]
#[case::entry_with_backlink_and_skiplink(
    crate::test_utils::fixtures::defaults::entry_with_backlink_and_skiplink()
)]
fn many_valid_entries(#[case] entry: Entry, key_pair: KeyPair) {}

// This template contains various types of valid operation.
#[template]
#[rstest]
#[allow(unused_qualifications)]
#[case::create_operation(crate::test_utils::fixtures::defaults::create_operation())]
#[allow(unused_qualifications)]
#[case::update_operation(crate::test_utils::fixtures::defaults::update_operation())]
#[allow(unused_qualifications)]
#[case::delete_operation(crate::test_utils::fixtures::defaults::delete_operation())]
fn all_operation_types(entry: Entry, #[case] operation: Operation, key_pair: KeyPair) {}

// Template which will contain many version fixtures in the future.
#[template]
#[rstest]
#[case::v0_3_0(crate::test_utils::fixtures::v0_3_0_fixture())]
fn version_fixtures(#[case] fixture: Fixture) {}

// Here we export the macros for use in the rest of the crate.
#[allow(unused_imports)]
pub(crate) use all_operation_types;
#[allow(unused_imports)]
pub(crate) use many_valid_entries;
#[allow(unused_imports)]
pub(crate) use non_default_operation_values_panic;
#[allow(unused_imports)]
pub(crate) use version_fixtures;
