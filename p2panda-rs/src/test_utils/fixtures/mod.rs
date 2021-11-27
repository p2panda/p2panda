// SPDX-License-Identifier: AGPL-3.0-or-later

//! `rstest` fixtures and templates which can be injected into tests
//!
//! From the `rstest` docs: "rstest uses procedural macros to help you on writing fixtures and table-based tests.
//! The core idea is that you can inject your test dependencies by passing them as test arguments."
//!
//! With templates you can apply many rstest cases to a single test. They utilize the
//! [rstest_reuse](https://github.com/la10736/rstest/tree/master/rstest_reuse) crate.
//!
//! <https://github.com/la10736/rstest>
//!
//! ## Example
//! ```
//! # extern crate p2panda_rs;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # #[cfg(test)]
//! # mod tests {
//! # use std::convert::TryFrom;
//! # use rstest::rstest;
//! # use rstest_reuse::apply;
//! # use crate::entry::{sign_and_encode, Entry};
//! # use crate::identity::KeyPair;
//! # use crate::message::{Message, MessageEncoded};
//! // **hidden imports used, see code for full import list**
//!
//! // These are the fixtures we will be using below
//! use crate::test_utils::fixtures::{create_message, defaults, entry, key_pair, Fixture};
//! // And these are the templates we can run tests against
//! use crate::test_utils::fixtures::templates::{
//!     many_valid_entries, non_default_message_values_panic, version_fixtures,
//! };
//!
//! // In this test `entry` and `key_pair` are injected directly into the test, they were imported from the
//! // fixtures module above.
//! #[rstest]
//! fn encode_entry(entry: Entry, key_pair: KeyPair) {
//!     assert!(sign_and_encode(&entry, &key_pair).is_ok());
//! }
//!
//! // Here `entry` and `key_pair` are still injected automatically but we also
//! // test against several different `message` value cases which are manually
//! // passed in via the #[case] macro. We can name the cases for nice test result printouts.
//! #[rstest]
//! // This case should pass as the default create message matches the content of the default entry
//! #[case::default_message(defaults::create_message())]
//! // This case should panic as we are passing in a non-default message value
//! #[should_panic] // panic macro flag
//! #[case::non_default_message(create_message(hash(DEFAULT_SCHEMA_HASH), message_fields(vec![("message", "Boo!")])))]
//! fn message_validation(entry: Entry, #[case] message: Message, key_pair: KeyPair) {
//!     let encoded_message = MessageEncoded::try_from(&message).unwrap();
//!     let signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();
//!     assert!(signed_encoded_entry
//!         .validate_message(&encoded_message)
//!         .is_ok());
//! }
//!
//! // This test is similar to the one seen above, but now uses a template to run
//! // the test against many non default message values. These are defined in
//! // fixtures/templates.rs. We also set a custom case which should pass.
//! #[apply(non_default_message_values_panic)]
//! #[case(defaults::create_message())]
//! fn message_validation_with_templates(
//!     entry: Entry,
//!     #[case] message: Message,
//!     key_pair: KeyPair,
//! ) {
//!     let encoded_message = MessageEncoded::try_from(&message).unwrap();
//!     let signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();
//!     assert!(signed_encoded_entry
//!         .validate_message(&encoded_message)
//!         .is_ok());
//! }
//!
//! // This test is similar to the first, but now using a template we can test
//! // against many different valid entries.
//! #[apply(many_valid_entries)]
//! fn encode_multiple_entries(#[case] entry: Entry, key_pair: KeyPair) {
//!     assert!(sign_and_encode(&entry, &key_pair).is_ok());
//! }
//!
//! // Finally we can run a test against all of our versioned p2panda fixture data
//! #[apply(version_fixtures)]
//! fn fixtures_sign_encode(#[case] fixture: Fixture) {
//!     // Sign and encode fixture Entry
//!     let entry_signed_encoded = sign_and_encode(&fixture.entry, &fixture.key_pair).unwrap();
//!
//!     // fixture EntrySigned hash should equal newly encoded EntrySigned hash.
//!     assert_eq!(
//!         fixture.entry_signed_encoded.hash().as_str(),
//!         entry_signed_encoded.hash().as_str()
//!     );
//! }
//! # }
//! # Ok(())
//! # }
//! ```

#[cfg(test)]
pub mod defaults;
#[cfg(test)]
mod fixtures;
#[cfg(test)]
pub mod templates;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub use fixtures::*;
