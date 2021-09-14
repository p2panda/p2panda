// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(test)]
mod tests {

use std::convert::TryFrom;

    use std::convert::TryFrom;

    use rstest::{fixture, rstest};

    use crate::entry::{decode_entry, sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::message::{Message, MessageEncoded, MessageFields, MessageValue};

    struct PandaTestFixture {
        entry_signed_encoded: EntrySigned,
        key_pair: KeyPair,
        message_encoded: MessageEncoded,
        entry: Entry,
    }

    fn create_message_fields(keys: Vec<&str>, values: Vec<&str>) -> MessageFields {
        let mut fields = MessageFields::new();
        for (pos, key) in keys.iter().enumerate() {
            fields
                .add(
                    key.to_owned(),
                    MessageValue::Text(values.get(pos).unwrap().to_string()),
                )
                .unwrap();
        }
        fields
    }

    #[fixture]
    fn key_pair() -> KeyPair {
        KeyPair::new()
    }

    #[fixture]
    fn message(
        #[default(vec!["message"])] keys: Vec<&str>,
        #[default(vec!["Hello!"])] values: Vec<&str>,
    ) -> Message {
        let fields = create_message_fields(keys, values);
        Message::new_create(Hash::new_from_bytes(vec![1, 2, 3]).unwrap(), fields).unwrap()
    }

    #[fixture]
    fn entry(
        message: Message,
        #[default(SeqNum::new(1).unwrap())] seq_num: SeqNum,
        #[default(None)] backlink: Option<Hash>,
        #[default(None)] skiplink: Option<Hash>,
    ) -> Entry {
        Entry::new(
            &LogId::default(),
            Some(&message),
            skiplink.as_ref(),
            backlink.as_ref(),
            &seq_num,
        )
        .unwrap()
    }

    #[fixture]
    fn v0_1_0_fixture() -> PandaTestFixture {
        PandaTestFixture {
            entry_signed_encoded: EntrySigned::new("009cdb3a8c0c4b308173d4c3c43a67a6d013444af99acb8be6c52423746d9aa2c10101f60040190c0d1b8a9bbe5d8b94c8226cdb5d9804af3af6a0c5e34c918864370953dbc7100438f1e5cb0f34bd214c595e37fbb0727f86e9f3eccafa9ba13ed8ef77a04ef01463f550ce62f983494d0eb6051c73a5641025f355758006724e5b730f47a4454c5395eab807325ee58d69c08d66461357d0f961aee383acc3247ed6419706").unwrap(),
            message_encoded: MessageEncoded::new("a466616374696f6e6663726561746566736368656d6178843030343031643736353636373538613562366266633536316631633933366438666338366235623432656132326162316461626634306432343964323764643930363430316664653134376535336634346331303364643032613235343931366265313133653531646531303737613934366133613063313237326239623334383433376776657273696f6e01666669656c6473a26b6465736372697074696f6ea26474797065637374726576616c756571666f7220706c6179696e67206368657373646e616d65a26474797065637374726576616c7565656368657373").unwrap(),
            key_pair: KeyPair::from_private_key(String::from("4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176")).unwrap(),
            entry: Entry::new(
                &LogId::new(1),
                Some(&Message::new_create(Hash::new("00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437").unwrap(), create_message_fields(vec!["name", "description"], vec!["chess", "for playing chess"])).unwrap()),
                None,
                None,
                &SeqNum::new(1).unwrap(),
            ).unwrap()
        }
    }

    // TODO: This test should be moved into EntrySigned once we have generalized test fixtures.
    #[rstest(message)]
    #[case(message(vec!["message"], vec!["Hello!"]))]
    #[should_panic]
    #[case(message(vec!["message"], vec!["Boo!"]))]
    #[should_panic]
    #[case(message(vec!["date"], vec!["2021-05-02T20:06:45.430Z"]))]
    #[should_panic]
    #[case(message(vec!["message", "date"], vec!["Hello!", "2021-05-02T20:06:45.430Z"]))]
    fn message_validation(entry: Entry, message: Message, key_pair: KeyPair) {
        let encoded_message = MessageEncoded::try_from(&message).unwrap();
        let signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();
        assert!(signed_encoded_entry
            .validate_message(&encoded_message)
            .is_ok());
    }

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
        let entry_decoded: Entry =
            decode_entry(&entry_first_encoded, Some(&message_encoded)).unwrap();

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
