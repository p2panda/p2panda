// SPDX-License-Identifier: AGPL-3.0-or-later

mod utils;
mod fixtures;

#[cfg(test)]
mod tests {

    use std::convert::TryFrom;

    use rstest::rstest;

    use p2panda_rs::entry::{decode_entry, sign_and_encode, Entry, LogId, SeqNum};
    use p2panda_rs::identity::KeyPair;
    use p2panda_rs::message::{Message, MessageEncoded};

    use crate::fixtures::{entry, key_pair, message, PandaTestFixture, v0_1_0_fixture};
    use crate::utils::MessageTypes;

    /// In this test the parameters `entry` and `key_pair` are injected from our test fixtures
    /// using the default values.
    #[rstest]
    fn entry_encoding_decoding(entry: Entry, key_pair: KeyPair) {
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

    #[rstest]
    fn sign_and_encode_roundtrip(entry: Entry, key_pair: KeyPair) {
        // Sign a p2panda entry. For this encoding, the entry is converted into a
        // bamboo-rs-core entry, which means that it also doesn't contain the message anymore
        let entry_first_encoded = sign_and_encode(&entry, &key_pair).unwrap();

        // Make an unsigned, decoded p2panda entry from the signed and encoded form. This is adding
        // the message back
        let message_encoded = MessageEncoded::try_from(entry.message().unwrap()).unwrap();
        let entry_decoded: Entry = decode_entry(&entry_first_encoded, Some(&message_encoded)).unwrap();

        // Re-encode the recovered entry to be able to check that we still have the same data
        let test_entry_signed_encoded = sign_and_encode(&entry_decoded, &key_pair).unwrap();
        assert_eq!(entry_first_encoded, test_entry_signed_encoded);

        // Create second p2panda entry without skiplink as it is not required
        let entry_second = Entry::new(
            &LogId::default(),
            entry.message(),
            None,
            Some(&entry_first_encoded.hash()),
            &SeqNum::new(2).unwrap(),
        )
        .unwrap();
        assert!(sign_and_encode(&entry_second, &key_pair).is_ok());
    }

    /// Here we inject the default value for `entry` and `key_pair`
    /// then test for different cases for `message`, many of which should all panic.
    #[rstest(message)]
    #[case(message(MessageTypes::Create, None, vec!["message"], vec!["Hello!"]))]
    #[should_panic]
    #[case(message(MessageTypes::Create, None, vec!["message"], vec!["Boo!"]))]
    #[should_panic]
    #[case(message(MessageTypes::Create, None, vec!["date"], vec!["2021-05-02T20:06:45.430Z"]))]
    #[should_panic]
    #[case(message(MessageTypes::Create, None, vec!["message", "date"], vec!["Hello!", "2021-05-02T20:06:45.430Z"]))]
    fn message_validation(entry: Entry, message: Message, key_pair: KeyPair) {
        let encoded_message = MessageEncoded::try_from(&message).unwrap();
        let signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();
        assert!(signed_encoded_entry
            .validate_message(&encoded_message)
            .is_ok());
    }

    /// Fixture tests
    /// These could be expanded with data from different p2panda versions
    #[rstest(fixture, case::v0_1_0(v0_1_0_fixture()))]
    fn fixture_sign_encode(fixture: PandaTestFixture) {
        // Sign and encode fixture Entry
        let entry_signed_encoded = sign_and_encode(&fixture.entry, &fixture.key_pair).unwrap();

        // fixture EntrySigned hash should equal newly encoded EntrySigned hash.
        assert_eq!(
            fixture.entry_signed_encoded.hash().as_str(),
            entry_signed_encoded.hash().as_str()
        );
    }

    #[rstest(fixture, case::v0_1_0(v0_1_0_fixture()))]
    fn fixture_decode_message(fixture: PandaTestFixture) {
        // Decode fixture MessageEncoded
        let message = Message::try_from(&fixture.message_encoded).unwrap();
        let message_fields = message.fields().unwrap();

        let fixture_message_fields = fixture.entry.message().unwrap().fields().unwrap();

        // Decoded fixture MessageEncoded values should match fixture Entry message values
        // Would be an improvement if we iterate over fields instead of using hard coded keys
        assert_eq!(
            message_fields.get("description").unwrap(),
            fixture_message_fields.get("description").unwrap()
        );

        assert_eq!(
            message_fields.get("name").unwrap(),
            fixture_message_fields.get("name").unwrap()
        );
    }

    #[rstest(fixture, case::v0_1_0(v0_1_0_fixture()))]
    fn fixture_decode_entry(fixture: PandaTestFixture) {
        // Decode fixture EntrySigned
        let entry = decode_entry(
            &fixture.entry_signed_encoded,
            Some(&fixture.message_encoded),
        )
        .unwrap();

        // Decoded Entry values should match fixture Entry values
        assert_eq!(entry.message().unwrap(), fixture.entry.message().unwrap());
        assert_eq!(entry.seq_num(), fixture.entry.seq_num());
        assert_eq!(entry.backlink_hash(), fixture.entry.backlink_hash());
        assert_eq!(entry.skiplink_hash(), fixture.entry.skiplink_hash());
        assert_eq!(entry.log_id(), fixture.entry.log_id());
    }

}    
fn main() {
    todo!()
}
