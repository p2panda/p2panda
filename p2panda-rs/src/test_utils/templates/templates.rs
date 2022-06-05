// SPDX-License-Identifier: AGPL-3.0-or-later

//! With these templates you can apply many rstest cases to a single test. They utilise the
//! somewhat experimental [rstest_reuse] crate.
//!
//! [tstest_reuse]: https://github.com/la10736/rstest/tree/master/rstest_reuse
use rstest_reuse::template;

/// This template contains various types of valid entries.
#[template]
#[rstest]
#[allow(unused_qualifications)]
#[case::first_entry(crate::test_utils::templates::defaults::first_entry())]
#[allow(unused_qualifications)]
#[case::entry_with_backlink(crate::test_utils::templates::defaults::entry_with_backlink())]
#[allow(unused_qualifications)]
#[case::entry_with_backlink_and_skiplink(
    crate::test_utils::templates::defaults::entry_with_backlink_and_skiplink()
)]
fn many_valid_entries(#[case] entry: Entry) {}

/// This template contains various types of valid meta-operation.
#[template]
#[rstest]
#[allow(unused_qualifications)]
#[case::create_meta_operation(crate::test_utils::templates::defaults::create_meta_operation())]
#[allow(unused_qualifications)]
#[case::update_meta_operation(crate::test_utils::templates::defaults::update_meta_operation())]
#[allow(unused_qualifications)]
#[case::delete_meta_operation(crate::test_utils::templates::defaults::delete_meta_operation())]
fn all_meta_operation_types(#[case] operation_with_meta: impl OperationWithMeta) {}

/// This template contains examples of all structs which implement the `AsOperation` trait.
#[template]
#[rstest]
#[allow(unused_qualifications)]
#[case::create_meta_operation(crate::test_utils::templates::defaults::create_meta_operation())]
#[allow(unused_qualifications)]
#[case::update_meta_operation(crate::test_utils::templates::defaults::update_meta_operation())]
#[allow(unused_qualifications)]
#[case::delete_meta_operation(crate::test_utils::templates::defaults::delete_meta_operation())]
#[allow(unused_qualifications)]
#[case::create_operation(crate::test_utils::templates::defaults::create_operation())]
#[allow(unused_qualifications)]
#[case::update_operation(crate::test_utils::templates::defaults::update_operation())]
#[allow(unused_qualifications)]
#[case::delete_operation(crate::test_utils::templates::defaults::delete_operation())]
fn implements_as_operation(#[case] operation: impl AsOperation) {}

/// Template which will contain many version fixtures in the future.
#[template]
#[rstest]
#[case::v0_3_0(crate::test_utils::fixtures::v0_3_0_fixture())]
fn version_fixtures(#[case] fixture: Fixture) {}

// Here we export the macros for use in the rest of the crate.
#[allow(unused_imports)]
pub(crate) use all_meta_operation_types;
#[allow(unused_imports)]
pub(crate) use implements_as_operation;
#[allow(unused_imports)]
pub(crate) use many_valid_entries;
#[allow(unused_imports)]
pub(crate) use version_fixtures;
