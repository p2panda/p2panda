// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests demonstrating the use of fixtures and templates.
use std::convert::TryFrom;

// Import rstest for infecting fixtures
use rstest::rstest;
// .. and rstest_reuse::apply module for using templates
use rstest_reuse::apply;

// Import p2panda types and methods we will be using in the tests
use crate::entry::{sign_and_encode, Entry};
use crate::identity::KeyPair;
use crate::operation::{Operation, OperationEncoded, OperationValue};
// Import the fixtures we will be using
use crate::test_utils::fixtures::{create_operation, defaults, entry, key_pair, Fixture};
// Import the templates we want to run tests against
use crate::test_utils::fixtures::templates::{
    many_valid_entries, non_default_operation_values_panic, version_fixtures,
};
// Import dependencies for the templates module
use crate::schema::SchemaId;
use crate::test_utils::constants::TEST_SCHEMA_ID;
use crate::test_utils::fixtures::operation_fields;

// In this test `entry` and `key_pair` are injected directly into the test, they were imported from
// the fixtures module above.
#[rstest]
fn encode_entry(entry: Entry, key_pair: KeyPair) {
    assert!(sign_and_encode(&entry, &key_pair).is_ok());
}

// Here `entry` and `key_pair` are still injected automatically and we also test against several
// different operation value cases which are manually passed in via the `#[case]` macro. We can
// name the cases for nice test result printouts.
#[rstest]
// This case should pass as the default CREATE operation matches the content of the default entry.
#[case::default_operation(defaults::create_operation())]
// This case should panic as we are passing in a non-default operation value.
#[should_panic] // panic macro flag
#[case::non_default_operation(create_operation(SchemaId::new(TEST_SCHEMA_ID).unwrap(), operation_fields(vec![("message", OperationValue::Text("Boo!".to_string()))])))]
fn operation_validation(entry: Entry, #[case] operation: Operation, key_pair: KeyPair) {
    let encoded_operation = OperationEncoded::try_from(&operation).unwrap();
    let signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();
    assert!(signed_encoded_entry
        .validate_operation(&encoded_operation)
        .is_ok());
}

// This test is similar to the one seen above, but now uses a template to run the test against many
// non default operation values. These are defined in `fixtures/templates.rs`. We also set a custom
// case which should pass.
#[apply(non_default_operation_values_panic)]
#[case(defaults::create_operation())]
fn operation_validation_with_templates(
    entry: Entry,
    #[case] operation: Operation,
    key_pair: KeyPair,
) {
    let encoded_operation = OperationEncoded::try_from(&operation).unwrap();
    let signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();
    assert!(signed_encoded_entry
        .validate_operation(&encoded_operation)
        .is_ok());
}

// This test is similar to the first, but now using a template we can test against many different
// valid entries.
#[apply(many_valid_entries)]
fn encode_multiple_entries(#[case] entry: Entry, key_pair: KeyPair) {
    assert!(sign_and_encode(&entry, &key_pair).is_ok());
}

// Finally we can run a test against all of our versioned p2panda fixture data.
#[apply(version_fixtures)]
fn fixtures_sign_encode(#[case] fixture: Fixture) {
    // Sign and encode fixture Entry
    let entry_signed_encoded = sign_and_encode(&fixture.entry, &fixture.key_pair).unwrap();

    // Fixture EntrySigned hash should equal newly encoded EntrySigned hash.
    assert_eq!(
        fixture.entry_signed_encoded.hash().as_str(),
        entry_signed_encoded.hash().as_str()
    );
}
