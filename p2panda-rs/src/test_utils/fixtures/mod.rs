// SPDX-License-Identifier: AGPL-3.0-or-later

//! General purpose fixtures which can be injected into test methods as parameters.
//!
//! The fixtures can optionally be passed in with custom parameters which overrides the default
//! values. See examples for more details.
//!
//! Implemented using the [`rstest`](https://github.com/la10736/rstest) library.
//!
//! ## Example
//!
//! ```
//! # extern crate p2panda_rs;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # #[cfg(test)]
//! # mod tests {
//! use rstest::rstest;
//! use p2panda_rs::test_utils::fixtures::{entry, entry_auto_gen_links, operation};
//! use p2panda_rs::operation::{AsOperation, Operation, OperationValue};
//! use p2panda_rs::entry::Entry;
//! use p2panda_rs::test_utils::constants::{test_fields, SCHEMA_ID};
//!
//! #[rstest]
//! fn inserts_the_default_entry(entry: Entry) {
//!     assert_eq!(entry.seq_num().as_u64(), 1)
//! }
//!
//! #[rstest]
//! fn just_change_the_log_id(#[with(1, 2)] entry: Entry) {
//!     assert_eq!(entry.seq_num().as_u64(), 2)
//! }
//!
//! #[rstest]
//! #[case(entry(1, 1, None, None, None))]
//! #[should_panic]
//! #[case(entry(0, 1, None, None, None))]
//! #[should_panic]
//! #[case::panic(entry(1, 1, Some(HASH.parse().unwrap()), None, None))]
//! fn different_cases_pass_or_panic(#[case] _entry: Entry) {}
//!
//! #[rstest]
//! fn just_change_the_seq_num(
//!     #[from(entry_auto_gen_links)]
//!     #[with(30)] // This seq_num should have a backlink and skiplink
//!     entry: Entry,
//! ) {
//!     assert_eq!(entry.seq_num().as_u64(), 30);
//!     assert_eq!(entry.log_id().as_u64(), 1);
//!     assert!(entry.backlink_hash().is_some());
//!     assert!(entry.skiplink_hash().is_some())
//! }
//!
//! // The fixtures can also be used as a constructor within the test itself.
//! //
//! // Here we combine that functionality with another `rstest` feature `#[value]`. This test runs once for
//! // every combination of values provided.
//! #[rstest]
//! fn used_as_constructor(#[values(1, 2, 3, 4)] seq_num: u64, #[values(1, 2, 3, 4)] log_id: u64) {
//!     let entry = entry_auto_gen_links(seq_num, log_id);
//!
//!     assert_eq!(entry.seq_num().as_u64(), seq_num);
//!     assert_eq!(entry.log_id().as_u64(), log_id)
//! }
//!
//! #[rstest]
//! fn insert_default_operation(operation: Operation) {
//!     assert_eq!(
//!         *operation.fields().unwrap().get("username").unwrap(),
//!         OperationValue::Text("bubu".to_string())
//!     )
//! }
//!
//! #[rstest]
//! fn change_just_the_fields(
//!     #[with(Some(operation_fields(vec![("username", OperationValue::Text("panda".to_string()))])))]
//!     operation: Operation,
//! ) {
//!     assert_eq!(
//!         *operation.fields().unwrap().get("username").unwrap(),
//!         OperationValue::Text("panda".to_string())
//!     )
//! }
//!
//! #[rstest]
//! #[case(operation(Some(operation_fields(test_fields())), None, None))] // if no schema is passed, the default is chosen
//! #[case(operation(Some(operation_fields(test_fields())), None, Some(SCHEMA_ID.parse().unwrap())))]
//! #[case(operation(Some(operation_fields(test_fields())), None, Some("schema_definition_v1".parse().unwrap())))]
//! #[should_panic]
//! #[case(operation(Some(operation_fields(test_fields())), None, Some("not_a_schema_string".parse().unwrap())))]
//! fn operations_with_different_schema(#[case] _operation: Operation) {}
//!
//! # }
//! # Ok(())
//! # }
//! ```
mod document_fixtures;
mod entry_fixtures;
mod hash_fixtures;
mod identity_fixtures;
mod operation_fixtures;
mod schema_fixtures;
mod version_fixtures;

pub use document_fixtures::*;
pub use entry_fixtures::*;
pub use hash_fixtures::*;
pub use identity_fixtures::*;
pub use operation_fixtures::*;
pub use schema_fixtures::*;
pub use version_fixtures::*;
