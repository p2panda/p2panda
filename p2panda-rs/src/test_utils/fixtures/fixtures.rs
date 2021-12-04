// SPDX-License-Identifier: AGPL-3.0-or-later

/// General purpose fixtures which can be injected into rstest methods as parameters.
///
/// The fixtures can optionally be passed in with custom parameters which overides the default values.
use rstest::fixture;

use crate::entry::{Entry, EntrySigned, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::message::{Message, MessageEncoded, MessageFields};
use crate::test_utils::utils::{self, DEFAULT_HASH, DEFAULT_PRIVATE_KEY, DEFAULT_SCHEMA_HASH};

/// Fixture struct which contains versioned p2panda data for testing
#[derive(Debug)]
pub struct Fixture {
    pub entry: Entry,
    pub entry_signed_encoded: EntrySigned,
    pub key_pair: KeyPair,
    pub message_encoded: MessageEncoded,
}

/// Fixture which injects the default private key string into a test method
#[fixture]
pub fn private_key() -> String {
    DEFAULT_PRIVATE_KEY.into()
}

/// Fixture which injects the default KeyPair into a test method. Default value can be overridden at testing
/// time by passing in a custom private key string.
#[fixture]
pub fn key_pair(private_key: String) -> KeyPair {
    utils::keypair_from_private(private_key)
}

/// Fixture which injects the default SeqNum into a test method. Default value can be overridden at testing
/// time by passing in a custom seq num as i64.
#[fixture]
pub fn seq_num(#[default(1)] n: i64) -> SeqNum {
    utils::seq_num(n)
}

/// Fixture which injects the default schema Hash into a test method. Default value can be overridden at testing
/// time by passing in a custom schema hash string.
#[fixture]
pub fn schema(#[default(DEFAULT_SCHEMA_HASH)] schema_str: &str) -> Hash {
    utils::hash(schema_str)
}

/// Fixture which injects the default Hash into a test method. Default value can be overridden at testing
/// time by passing in a custom hash string.
#[fixture]
pub fn hash(#[default(DEFAULT_HASH)] hash_str: &str) -> Hash {
    utils::hash(hash_str)
}

/// Fixture which injects the default MessageFields value into a test method. Default value can be overridden at testing
/// time by passing in a custom vector of key-value tuples.
#[fixture]
pub fn fields(
    #[default(vec![("message", "Hello!")])] fields_vec: Vec<(&str, &str)>,
) -> MessageFields {
    utils::message_fields(fields_vec)
}

/// Fixture which injects the default Entry into a test method. Default value can be overridden at testing
/// time by passing in custom message, seq number, backlink and skiplink.
#[fixture]
pub fn entry(
    message: Message,
    seq_num: SeqNum,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
) -> Entry {
    utils::entry(message, skiplink, backlink, seq_num)
}

/// Fixture which injects the default Message into a test method. Default value can be overridden at testing
/// time by passing in custom message fields and instance id.
#[fixture]
pub fn message(
    #[default(Some(fields(vec![("message", "Hello!")])))] fields: Option<MessageFields>,
    #[default(None)] instance_id: Option<Hash>,
) -> Message {
    utils::any_message(fields, instance_id)
}

/// Fixture which injects the default Hash into a test method as an Option. Default value can be overridden at testing
/// time by passing in custom hash string.
#[fixture]
pub fn some_hash(#[default(DEFAULT_HASH)] str: &str) -> Option<Hash> {
    let hash = Hash::new(str);
    Some(hash.unwrap())
}

/// Fixture which injects the default CREATE Message into a test method. Default value can be overridden at testing
/// time by passing in custom schema hash and message fields.
#[fixture]
pub fn create_message(schema: Hash, fields: MessageFields) -> Message {
    utils::create_message(schema, fields)
}

/// Fixture which injects the default UPDATE Message into a test method. Default value can be overridden at testing
/// time by passing in custom schema hash, instance id hash and message fields.
#[fixture]
pub fn update_message(
    schema: Hash,
    #[from(hash)] instance_id: Hash,
    #[default(fields(vec![("message", "Updated, hello!")]))] fields: MessageFields,
) -> Message {
    utils::update_message(schema, instance_id, fields)
}

/// Fixture which injects the default DELETE Message into a test method. Default value can be overridden at testing
/// time by passing in custom schema hash and instance id hash.
#[fixture]
pub fn delete_message(schema: Hash, #[from(hash)] instance_id: Hash) -> Message {
    utils::delete_message(schema, instance_id)
}

/// Fixture which injects versioned p2panda testing data into a test method.
#[fixture]
pub fn v0_1_0_fixture() -> Fixture {
    let message_fields = utils::message_fields(vec![
        ("name", "chess"),
        ("description", "for playing chess"),
    ]);
    let message = create_message(Hash::new(DEFAULT_SCHEMA_HASH).unwrap(), message_fields);

    Fixture {
        entry_signed_encoded: EntrySigned::new("009cdb3a8c0c4b308173d4c3c43a67a6d013444af99acb8be6c52423746d9aa2c10101b600203f6bc1247808c9e367a63a2142353f91bfe8f155b9350587095bc3f44e3958125e9404c343167f9479c1e94dbdbe7397f1f1af244333c95b0e15bca4d9728a0309da96e71c16900096bf61ceb36d82584d0226537f3ebe7c79e25719c2645e07").unwrap(),
        message_encoded: MessageEncoded::new("a466616374696f6e6663726561746566736368656d61784430303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696f6e02666669656c6473a26b6465736372697074696f6ea26474797065637374726576616c756571666f7220706c6179696e67206368657373646e616d65a26474797065637374726576616c7565656368657373").unwrap(),
        key_pair: utils::keypair_from_private("4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176".into()),
        entry: entry(message, seq_num(1), None, None)
    }
}
