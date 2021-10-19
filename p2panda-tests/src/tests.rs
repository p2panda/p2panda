use rstest::*;
use std::convert::TryFrom;

use rstest::rstest;
use rstest_reuse::apply;

use p2panda_rs::entry::{decode_entry, sign_and_encode, Entry, LogId, SeqNum};
use p2panda_rs::identity::KeyPair;
use p2panda_rs::message::{Message, MessageEncoded};

use crate::fixtures::*;
use crate::templates::{many_entry_versions, messages_not_matching_entry_should_fail};
use crate::utils::Fixture;

/// In this test `key_pair` is injected directly from our test fixtures and `entry`
/// is tested agains all cases on the `many_entry_versions` template.
#[apply(many_entry_versions)]
fn entry_encoding_decoding(#[case] entry: Entry, key_pair: KeyPair) {
    // Encode Message
    let encoded_message = MessageEncoded::try_from(entry.message().unwrap()).unwrap();

    // Sign and encode Entry
    let signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();

    // Decode signed and encoded Entry
    let decoded_entry = decode_entry(&signed_encoded_entry, Some(&encoded_message)).unwrap();

    // All Entry and decoded Entry values should be equal
    assert_eq!(entry.log_id(), decoded_entry.log_id());
    assert_eq!(entry.message().unwrap(), decoded_entry.message().unwrap());
    assert_eq!(entry.seq_num(), decoded_entry.seq_num());
    assert_eq!(entry.backlink_hash(), decoded_entry.backlink_hash());
    assert_eq!(entry.skiplink_hash(), decoded_entry.skiplink_hash());
}

/// In this test `entry` and `key_pair` are injected directly from our test fixtures and `message`
/// is tested agains all cases on the `messages_not_matching_entry_should_fail` and one manually defined passing case.
#[apply(messages_not_matching_entry_should_fail)]
#[case(message_hello())]
fn message_validation(entry: Entry, #[case] message: Message, key_pair: KeyPair) {
    let encoded_message = MessageEncoded::try_from(&message).unwrap();
    let signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();
    assert!(signed_encoded_entry
        .validate_message(&encoded_message)
        .is_ok());
}

/// Fixture tests
/// These could be expanded with data from different p2panda versions
/// The fixture parameter is injected directly and renamed with the [#from] macro
#[rstest]
fn fixture_sign_encode(#[from(v0_1_0_fixture)] fixture: Fixture) {
    // Sign and encode fixture Entry
    let entry_signed_encoded = sign_and_encode(&fixture.entry, &fixture.key_pair).unwrap();

    // fixture EntrySigned hash should equal newly encoded EntrySigned hash.
    assert_eq!(
        fixture.entry_signed_encoded.hash().as_str(),
        entry_signed_encoded.hash().as_str()
    );
}
