use std::convert::{TryFrom, TryInto};

use bamboo_rs_core::entry::{decode, MAX_ENTRY_SIZE};
use bamboo_rs_core::{Entry as BambooEntry, Signature as BambooSignature, YamfHash};

use crate::atomic::{Entry, EntrySigned, Hash, LogId, Message, MessageEncoded, SeqNum};
use crate::atomic::error::EntrySignedError;
use crate::key_pair::KeyPair;
use crate::atomic::Blake2BArrayVec;
use arrayvec::ArrayVec;
use ed25519_dalek::PublicKey;


/// Takes an [`EntrySigned`] and a [`MessageEncoded`], validates the encoded message against the entry payload hash, 
/// returns the decoded message in the form of a [`Message`] if valid otherwise throws an error.
pub fn validate_message(entry_encoded: &EntrySigned, message_encoded: &MessageEncoded) -> Result<Message, EntrySignedError> {
    // Convert to Entry from bamboo_rs_core first
    let entry: BambooEntry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> = entry_encoded.into();
    // Messages may be omitted because the entry still contains the message hash. If the
    // message is explicitly included we require its hash to match.
    let message = match message_encoded {
        msg => {
            let yamf_hash: YamfHash<Blake2BArrayVec> =
                (&msg.hash()).to_owned().into();

            if yamf_hash != entry.payload_hash {
                return Err(EntrySignedError::MessageHashMismatch);
            }
            Message::from(msg)
        }
    };
    Ok(message)
}

/// Takes an [`Entry`] and a public key, returns a tuple containing encoded entry bytes and their size.
pub fn encode_entry(entry: &Entry, public_key: &Box<[u8]>) -> Result<(usize, [u8; MAX_ENTRY_SIZE]), EntrySignedError> {
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
    let entry: BambooEntry<_, &[u8]> = BambooEntry {
        log_id: entry.log_id().as_i64() as u64,
        is_end_of_feed: false,
        payload_hash: message_hash.into(),
        payload_size: message_size as u64,
        author: PublicKey::from_bytes(public_key)?,
        seq_num: entry.seq_num().as_i64() as u64,
        backlink,
        lipmaa_link,
        sig: None,
    };

    let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];
    
    // Get unsigned entry bytes
    let entry_size = entry.encode(&mut entry_bytes)?;
    Ok((entry_size, entry_bytes))
}

/// Takes unsigned entry bytes and their size and a [`KeyPair`], returns a tuple containing signed and encoded entry bytes and their size.
pub fn sign_entry(entry_bytes: [u8; MAX_ENTRY_SIZE], unsigned_entry_size: usize, key_pair: &KeyPair) -> Result<(usize, [u8; MAX_ENTRY_SIZE]), EntrySignedError>{
    // Make copy of entry_bytes before passing to decode
    let mut entry_bytes_copy = entry_bytes.clone();
    
    // Decode unsigned entry bytes
    let mut entry = decode(&entry_bytes)?;
    
    // Sign and add signature to entry
    let sig_bytes = key_pair.sign(&entry_bytes_copy[..unsigned_entry_size]);
    let signature = BambooSignature(&*sig_bytes);
    entry.sig = Some(signature);

    // Get signed entry bytes
    let signed_entry_size = entry.encode(&mut entry_bytes_copy)?;
    Ok((signed_entry_size, entry_bytes_copy))
}

/// Takes an [`Entry`] and a [`KeyPair`], returns signed and encoded entry bytes in form of an
/// [`EntrySigned`] instance.
///
/// After signing the result is ready to be sent to a p2panda node.
pub fn sign_and_encode(entry: &Entry, key_pair: &KeyPair) -> Result<EntrySigned, EntrySignedError> {

    // Get unsigned entry bytes
    let (unsigned_entry_size, unsigned_entry_bytes) = encode_entry(entry, &key_pair.public_key_bytes())?;
    
    // Sign entry and get signed entry bytes
    let (signed_entry_size, signed_entry_bytes) = sign_entry(unsigned_entry_bytes, unsigned_entry_size, key_pair)?;
    
    // Return signed entry bytes in the form of an EntrySigned
    EntrySigned::try_from(&signed_entry_bytes[..signed_entry_size])
}

/// Takes [`EntrySigned`] and optionally [`MessageEncoded`] as arguments, returns a decoded and unsigned [`Entry`]. When a [`MessageEncoded`] is passed
/// it will automatically check its integrity with this [`Entry`] by comparing their hashes. Valid messages will be included in the returned 
/// [`Entry`], if an invalid message is passed an error will be returned.
/// 
/// Entries are separated from the messages they refer to. Since messages can independently be
/// deleted they can be passed on as an optional argument. When a [`Message`] is passed
/// it will automatically check its integrity with this Entry by comparing their hashes.
pub fn decode_entry(entry_encoded: &EntrySigned, message_encoded: Option<&MessageEncoded>) -> Result<Entry, EntrySignedError> {
    // Convert to Entry from bamboo_rs_core first
    let entry: BambooEntry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> = entry_encoded.into();

    let message = match message_encoded {
        Some(msg) => Some(validate_message(entry_encoded, msg)?),
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
    ).unwrap())
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use crate::atomic::{EntrySigned, MessageEncoded, Message, MessageValue, SeqNum};
    use crate::key_pair::KeyPair;
    use crate::test_helpers::{mock_message, mock_first_entry, mock_entry};

    use super::{decode_entry, sign_and_encode, validate_message};

    #[test]
    fn message_validation() {
        // Prepare test values
        let key_pair = KeyPair::new();
        let message = mock_message(String::from("Hello!"));
        let encoded_message = MessageEncoded::try_from(&message).unwrap();
        let entry = mock_first_entry(message);
        let signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();
        
        // Correct message should pass validation 
        assert!(validate_message(&signed_encoded_entry, &encoded_message).is_ok());
        
        // A message with different content should fail validation
        let bad_message = mock_message(String::from("Boo!"));
        let bad_encoded_message = MessageEncoded::try_from(&bad_message).unwrap();
        
        assert!(validate_message(&signed_encoded_entry, &bad_encoded_message).is_err());
    }
    #[test]
    fn entry_signing_and_encoding() {
        let key_pair = KeyPair::new();

        // Prepare test values for first entry
        let message = mock_message(String::from("Hello!"));
        let encoded_message = MessageEncoded::try_from(&message).unwrap();
        let entry = mock_first_entry(message);
        
        // Sign and encode Entry
        let signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();
        
        // Decode signed and encoded Entry
        let decoded_entry = decode_entry(&signed_encoded_entry, Some(&encoded_message)).unwrap();
        
        // All Entry and decoded Entry params should be equal 
        assert_eq!(entry.log_id(), decoded_entry.log_id());
        assert_eq!(entry.message().unwrap(), decoded_entry.message().unwrap());
        assert_eq!(entry.seq_num(), decoded_entry.seq_num());
        assert_eq!(entry.backlink_hash(), decoded_entry.backlink_hash());
        assert_eq!(entry.skiplink_hash(), decoded_entry.skiplink_hash());
        
        // Prepare test values for second entry
        let second_message = mock_message(String::from("Another hello!"));
        let second_encoded_message = MessageEncoded::try_from(&second_message).unwrap();
        let second_entry = mock_entry(second_message, Some(signed_encoded_entry), None, 2);
        
        // Sign and encode second Entry
        let second_signed_encoded_entry = sign_and_encode(&second_entry, &key_pair).unwrap();

        // Decode signed and encoded second Entry
        let second_decoded_entry = decode_entry(
            &second_signed_encoded_entry, 
            Some(&second_encoded_message)
        ).unwrap();
        
        // All decoded_entry and second_decoded_entry Entry params should not be equal
        // except for LogId (1) and skiplink_hash (None) 
        assert_eq!(decoded_entry.log_id(), second_decoded_entry.log_id());
        assert_ne!(decoded_entry.message().unwrap(), second_decoded_entry.message().unwrap());
        assert_ne!(decoded_entry.seq_num(), second_decoded_entry.seq_num());
        assert_ne!(decoded_entry.backlink_hash(), second_decoded_entry.backlink_hash());
        assert_eq!(decoded_entry.skiplink_hash(), second_decoded_entry.skiplink_hash());
    }
    #[test]
    fn entry_signing_and_encoding_fixtures() {
        let public_key = "ba07a8da75dd2f922d62eae7e7ac7c081e06bf0c192b2d8ea1b2ab5e9c59013e";
        let private_key = "31f33f8e6c262f36a0e5397348093a459d66d8cb5946798ad62d5eb8e7645bdb";
        let author = "ba07a8da75dd2f922d62eae7e7ac7c081e06bf0c192b2d8ea1b2ab5e9c59013e";
        let entry_bytes= "00ba07a8da75dd2f922d62eae7e7ac7c081e06bf0c192b2d8ea1b2ab5e9c59013e01040040944b4ae2ff31d0adc13cf94ba43b766871b4e56e96d0eebbc1b9e2b8226d448e8bc1f9507a21894579578491ff778a008688c2a3e8a409fc37522d9eabaa114c004054f65f3ac2ccf13f5862eb7c29ac20e830e173d062416dfd03a27e8a2315b69f402cfa4ca741d243b184b1d8ff203cf1f1ec4619f44758263f19a75a3537e780ee00408960c9d4f864aef757d440bc5aa5a5c0d726312eddadad68f25d06fedd10f755d51a87565972f8c3d77ef7ac66531227131b0d8857fef749c3a98cfffae8519d1e8bdb78a27348232671acda6c16aca26148642b0e803e6e2e4dfc01ca0d46ea19546be7b4302b826363a6caa28fced7ef9fd847b35a49eb67b885d65af14305";
        let entry_hash = "004073412203af4eedbddde3a8183647f3c788667b4e693ebdd38830eeaef10cbdf956587f0f571213ba5a86b2cbc5926c8b6b27e5f721a69ced281aa8ad57aa4404";
        let log_id: i64 = 1;
        let payload_bytes = "a466616374696f6e6663726561746566736368656d6178843030343063663934663664363035363537653930633534336230633931393037306364616166373230396335653165613538616362386633353638666132313134323638646339616333626166653132616632373764323836666365376463353962376330633334383937336334653964616362653739343835653536616332613730326776657273696f6e01666669656c6473a26464617465a164546578747818323032312d30352d30325432303a30363a34352e3433305a676d657373616765a164546578746d477574656e204d6f7267656e21";
        let payload_hash = "00408960c9d4f864aef757d440bc5aa5a5c0d726312eddadad68f25d06fedd10f755d51a87565972f8c3d77ef7ac66531227131b0d8857fef749c3a98cfffae8519d";
        let seq_num: i64 = 4;
        let skiplink_hash = "0040944b4ae2ff31d0adc13cf94ba43b766871b4e56e96d0eebbc1b9e2b8226d448e8bc1f9507a21894579578491ff778a008688c2a3e8a409fc37522d9eabaa114c";
        let backlink_hash = "004054f65f3ac2ccf13f5862eb7c29ac20e830e173d062416dfd03a27e8a2315b69f402cfa4ca741d243b184b1d8ff203cf1f1ec4619f44758263f19a75a3537e780";
        
        // Create MessageEncoded from payload_bytes
        let encoded_message = MessageEncoded::new(&payload_bytes).unwrap();
        
        // Encoded message hash should equal payload_hash
        assert_eq!(encoded_message.hash().as_hex(), payload_hash.to_owned());
        
        // Decode MessageEncoded
        let message = Message::try_from(&encoded_message).unwrap();
        let message_fields = message.fields().unwrap();
        let message_value = message_fields.get("message").unwrap();
        let date_value = message_fields.get("date").unwrap();

        // Decoded message content should match correct values
        assert_eq!(message_value.to_owned(), MessageValue::Text("Guten Morgen!".to_owned()));
        assert_eq!(date_value.to_owned(), MessageValue::Text("2021-05-02T20:06:45.430Z".to_owned()));
        
        // Decoded message content should NOT match incorrect values
        assert_ne!(message_value.to_owned(), MessageValue::Text("Guten Abend!".to_owned()));
        assert_ne!(date_value.to_owned(), MessageValue::Text("2221-05-02T20:06:45.430Z".to_owned()));
        
        // Decode entry_bytes
        let encoded_entry = EntrySigned::new(entry_bytes).unwrap();
        let entry = decode_entry(&encoded_entry, Some(&encoded_message)).unwrap();
        
        // Decoded entry values should equal correct values
        assert_eq!(entry.message().unwrap().to_owned(), message);
        assert_eq!(entry.seq_num().to_owned(), SeqNum::new(seq_num).unwrap());
        assert_eq!(entry.backlink_hash().unwrap().as_hex(), backlink_hash);
        assert_eq!(entry.skiplink_hash().unwrap().as_hex(), skiplink_hash);
        assert_eq!(entry.log_id().as_i64(), log_id);

        // Re-sign and encode them using matching KeyPair
        let key_pair = KeyPair::from_private_key(private_key.to_owned()).unwrap();
        let re_signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();
        
        // Public key and author should be correct
        assert_eq!(key_pair.public_key(), public_key.to_owned());
        assert_eq!(key_pair.public_key(), author.to_owned());

        // Re-signed entry values should equal original values
        assert_eq!(re_signed_encoded_entry.as_str(), entry_bytes.to_owned());
        assert_eq!(re_signed_encoded_entry.hash().as_hex(), entry_hash.to_owned());
    }
}
