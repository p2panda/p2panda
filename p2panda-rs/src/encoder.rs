use std::convert::{TryFrom, TryInto};

use arrayvec::ArrayVec;
use bamboo_rs_core::entry::MAX_ENTRY_SIZE;
use bamboo_rs_core::{Entry as BambooEntry, Signature as BambooSignature};
use ed25519_dalek::PublicKey;

use crate::atomic::error::EntrySignedError;
use crate::atomic::{Entry, EntrySigned, Hash, LogId, Message, MessageEncoded, SeqNum};
use crate::key_pair::KeyPair;

/// Takes an [`Entry`] and a [`KeyPair`], returns signed and encoded entry bytes in form of an
/// [`EntrySigned`] instance.
///
/// After signing the result is ready to be sent to a p2panda node.
pub fn sign_and_encode(entry: &Entry, key_pair: &KeyPair) -> Result<EntrySigned, EntrySignedError> {
    // Generate message hash
    let message_encoded = match entry.message() {
        Some(message) => MessageEncoded::try_from(message)?,
        None => return Err(EntrySignedError::MessageMissing),
    };
    let message_hash = message_encoded.hash();
    let message_size = message_encoded.size();

    // Convert entry links to bamboo-rs `YamfHash` type
    let backlink = entry.backlink_hash().map(|link| link.to_owned().into());
    let lipmaa_link = if entry.is_skiplink_required() {
        if entry.skiplink_hash().is_none() {
            return Err(EntrySignedError::SkiplinkMissing);
        }
        entry.skiplink_hash().map(|link| link.to_owned().into())
    } else {
        // Omit skiplink when it is the same as backlink, this saves us some bytes
        None
    };

    // Create bamboo entry. See: https://github.com/AljoschaMeyer/bamboo#encoding for encoding
    // details and definition of entry fields.
    let mut entry: BambooEntry<_, &[u8]> = BambooEntry {
        log_id: entry.log_id().as_i64() as u64,
        is_end_of_feed: false,
        payload_hash: message_hash.into(),
        payload_size: message_size as u64,
        author: PublicKey::from_bytes(&key_pair.public_key_bytes())?,
        seq_num: entry.seq_num().as_i64() as u64,
        backlink,
        lipmaa_link,
        sig: None,
    };

    let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];

    // Get unsigned entry bytes
    let entry_size = entry.encode(&mut entry_bytes)?;

    // Sign and add signature to entry
    let sig_bytes = key_pair.sign(&entry_bytes[..entry_size]);
    let signature = BambooSignature(&*sig_bytes);
    entry.sig = Some(signature);

    // Get signed entry bytes
    let signed_entry_size = entry.encode(&mut entry_bytes)?;

    // Return signed entry bytes in the form of an EntrySigned
    EntrySigned::try_from(&entry_bytes[..signed_entry_size])
}

/// Takes [`EntrySigned`] and optionally [`MessageEncoded`] as arguments, returns a decoded and
/// unsigned [`Entry`]. When a [`MessageEncoded`] is passed it will automatically check its
/// integrity with this [`Entry`] by comparing their hashes. Valid messages will be included in the
/// returned [`Entry`], if an invalid message is passed an error will be returned.
///
/// Entries are separated from the messages they refer to. Since messages can independently be
/// deleted they can be passed on as an optional argument. When a [`Message`] is passed it will
/// automatically check its integrity with this Entry by comparing their hashes.
pub fn decode_entry(
    entry_encoded: &EntrySigned,
    message_encoded: Option<&MessageEncoded>,
) -> Result<Entry, EntrySignedError> {
    // Convert to Entry from bamboo_rs_core first
    let entry: BambooEntry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> = entry_encoded.into();

    let message = match message_encoded {
        Some(msg) => Some(Message::from(&entry_encoded.validate_message(msg)?)),
        None => None,
    };

    let entry_hash_backlink: Option<Hash> = match entry.backlink {
        Some(link) => Some(link.try_into()?),
        None => None,
    };

    let entry_hash_skiplink: Option<Hash> = match entry.lipmaa_link {
        Some(link) => Some(link.try_into()?),
        None => None,
    };

    Ok(Entry::new(
        &LogId::new(entry.log_id as i64),
        message.as_ref(),
        entry_hash_skiplink.as_ref(),
        entry_hash_backlink.as_ref(),
        &SeqNum::new(entry.seq_num as i64).unwrap(),
    )
    .unwrap())
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rstest::{fixture, rstest};

    use crate::atomic::{
        Entry, EntrySigned, Hash, LogId, Message, MessageEncoded, MessageFields, MessageValue,
        SeqNum,
    };
    use crate::key_pair::KeyPair;

    use super::{decode_entry, sign_and_encode};

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
            entry_signed_encoded: EntrySigned::new("00ba07a8da75dd2f922d62eae7e7ac7c081e06bf0c192b2d8ea1b2ab5e9c59013e01040040944b4ae2ff31d0adc13cf94ba43b766871b4e56e96d0eebbc1b9e2b8226d448e8bc1f9507a21894579578491ff778a008688c2a3e8a409fc37522d9eabaa114c004054f65f3ac2ccf13f5862eb7c29ac20e830e173d062416dfd03a27e8a2315b69f402cfa4ca741d243b184b1d8ff203cf1f1ec4619f44758263f19a75a3537e780ee00408960c9d4f864aef757d440bc5aa5a5c0d726312eddadad68f25d06fedd10f755d51a87565972f8c3d77ef7ac66531227131b0d8857fef749c3a98cfffae8519d1e8bdb78a27348232671acda6c16aca26148642b0e803e6e2e4dfc01ca0d46ea19546be7b4302b826363a6caa28fced7ef9fd847b35a49eb67b885d65af14305").unwrap(),
            message_encoded: MessageEncoded::new("a466616374696f6e6663726561746566736368656d6178843030343063663934663664363035363537653930633534336230633931393037306364616166373230396335653165613538616362386633353638666132313134323638646339616333626166653132616632373764323836666365376463353962376330633334383937336334653964616362653739343835653536616332613730326776657273696f6e01666669656c6473a26464617465a164546578747818323032312d30352d30325432303a30363a34352e3433305a676d657373616765a164546578746d477574656e204d6f7267656e21").unwrap(),
            key_pair: KeyPair::from_private_key(String::from("31f33f8e6c262f36a0e5397348093a459d66d8cb5946798ad62d5eb8e7645bdb")).unwrap(),
            entry: Entry::new(
                &LogId::new(1),
                Some(&Message::new_create(Hash::new("0040cf94f6d605657e90c543b0c919070cdaaf7209c5e1ea58acb8f3568fa2114268dc9ac3bafe12af277d286fce7dc59b7c0c348973c4e9dacbe79485e56ac2a702").unwrap(), create_message_fields(vec!["message", "date"], vec!["Guten Morgen!", "2021-05-02T20:06:45.430Z"])).unwrap()),
                Some(&Hash::new("0040944b4ae2ff31d0adc13cf94ba43b766871b4e56e96d0eebbc1b9e2b8226d448e8bc1f9507a21894579578491ff778a008688c2a3e8a409fc37522d9eabaa114c").unwrap()),
                Some(&Hash::new("004054f65f3ac2ccf13f5862eb7c29ac20e830e173d062416dfd03a27e8a2315b69f402cfa4ca741d243b184b1d8ff203cf1f1ec4619f44758263f19a75a3537e780").unwrap()),
                &SeqNum::new(4).unwrap(),
            ).unwrap(),
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
            fixture.entry_signed_encoded.hash().as_hex(),
            entry_signed_encoded.hash().as_hex()
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
            message_fields.get("message").unwrap(),
            fixture_message_fields.get("message").unwrap()
        );

        assert_eq!(
            message_fields.get("date").unwrap(),
            fixture_message_fields.get("date").unwrap()
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
        assert_eq!(
            entry.backlink_hash().unwrap(),
            fixture.entry.backlink_hash().unwrap()
        );
        assert_eq!(
            entry.skiplink_hash().unwrap(),
            fixture.entry.skiplink_hash().unwrap()
        );
        assert_eq!(entry.log_id(), fixture.entry.log_id());
    }
}
