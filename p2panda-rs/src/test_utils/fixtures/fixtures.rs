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

/// Fixture which injects a random KeyPair into a test method.
#[fixture]
pub fn random_key_pair(private_key: String) -> KeyPair {
    utils::new_key_pair()
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
    const SCHEMA_HASH: &str = "00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437";

    let message_fields = utils::message_fields(vec![
        ("name", "chess"),
        ("description", "for playing chess"),
    ]);
    let message = create_message(hash(SCHEMA_HASH), message_fields);

    Fixture {
        entry_signed_encoded: EntrySigned::new("009cdb3a8c0c4b308173d4c3c43a67a6d013444af99acb8be6c52423746d9aa2c10101f60040190c0d1b8a9bbe5d8b94c8226cdb5d9804af3af6a0c5e34c918864370953dbc7100438f1e5cb0f34bd214c595e37fbb0727f86e9f3eccafa9ba13ed8ef77a04ef01463f550ce62f983494d0eb6051c73a5641025f355758006724e5b730f47a4454c5395eab807325ee58d69c08d66461357d0f961aee383acc3247ed6419706").unwrap(),
        message_encoded: MessageEncoded::new("a466616374696f6e6663726561746566736368656d6178843030343031643736353636373538613562366266633536316631633933366438666338366235623432656132326162316461626634306432343964323764643930363430316664653134376535336634346331303364643032613235343931366265313133653531646531303737613934366133613063313237326239623334383433376776657273696f6e01666669656c6473a26b6465736372697074696f6ea26474797065637374726576616c756571666f7220706c6179696e67206368657373646e616d65a26474797065637374726576616c7565656368657373").unwrap(),
        key_pair: utils::keypair_from_private("4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176".into()),
        entry: entry(message, seq_num(1), None, None)
    }
}
