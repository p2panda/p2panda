// SPDX-License-Identifier: AGPL-3.0-or-later
use rstest::fixture;

use p2panda_rs::entry::{Entry, EntrySigned, SeqNum};
use p2panda_rs::hash::Hash;
use p2panda_rs::identity::KeyPair;
use p2panda_rs::message::{Message, MessageEncoded};

use crate::utils::{Fixture, GENERIC_HASH, MESSAGE_SCHEMA};
use crate::Panda;

/// `rstest` fixtures which can be injected into tests
///
/// From the `rstest` docs: "rstest uses procedural macros to help you on writing fixtures and table-based tests.
/// The core idea is that you can inject your test dependencies by passing them as test arguments."
///
/// https://github.com/la10736/rstest

#[fixture]
pub fn key_pair() -> KeyPair {
    Panda::keypair()
}

#[fixture]
pub fn seq_num(#[default(1)] n: i64) -> SeqNum {
    Panda::seq_num(n)
}

#[fixture]
pub fn some_hash(#[default(GENERIC_HASH)] str: &str) -> Option<Hash> {
    Panda::some_hash(&str)
}

#[fixture]
pub fn some_default_hash() -> Option<Hash> {
    Panda::some_hash(GENERIC_HASH)
}

#[fixture]
pub fn message(
    #[default(Some(vec![("message", "Hello!")]))] fields: Option<Vec<(&str, &str)>>,
    #[default(None)] instance_id: Option<Hash>,
) -> Message {
    match fields {
        // It's a CREATE message
        Some(fields) if instance_id.is_none() => Panda::create_message(MESSAGE_SCHEMA, fields),
        // It's an UPDATE message
        Some(fields) => Panda::update_message(MESSAGE_SCHEMA, instance_id.unwrap(), fields),
        // It's a DELETE message
        None if instance_id.is_some() => {
            Panda::delete_message(MESSAGE_SCHEMA, instance_id.unwrap())
        }
        // It's a mistake....
        None => todo!(), // Error....
    }
}

#[fixture]
pub fn message_hello() -> Message {
    message(Some(vec![("message", "Hello!")]), None)
}

#[fixture]
pub fn create_message() -> Message {
    message(Some(vec![("message", "Hello!")]), None)
}

#[fixture]
pub fn update_message() -> Message {
    message(
        Some(vec![("message", "Updated, hello!")]),
        some_default_hash(),
    )
}

#[fixture]
pub fn delete_message() -> Message {
    message(None, some_default_hash())
}

#[fixture]
pub fn entry(
    message: Message,
    seq_num: SeqNum,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
) -> Entry {
    Panda::entry(&message, &seq_num, backlink, skiplink)
}

#[fixture]
pub fn first_entry() -> Entry {
    entry(message_hello(), seq_num(1), None, None)
}

#[fixture]
pub fn entry_with_backlink() -> Entry {
    entry(message_hello(), seq_num(2), some_default_hash(), None)
}

#[fixture]
pub fn entry_with_backlink_and_skiplink() -> Entry {
    entry(
        message_hello(),
        seq_num(13),
        some_default_hash(),
        some_default_hash(),
    )
}

#[fixture]
pub fn v0_1_0_fixture() -> Fixture {
    Fixture {
        entry_signed_encoded: EntrySigned::new("009cdb3a8c0c4b308173d4c3c43a67a6d013444af99acb8be6c52423746d9aa2c10101f60040190c0d1b8a9bbe5d8b94c8226cdb5d9804af3af6a0c5e34c918864370953dbc7100438f1e5cb0f34bd214c595e37fbb0727f86e9f3eccafa9ba13ed8ef77a04ef01463f550ce62f983494d0eb6051c73a5641025f355758006724e5b730f47a4454c5395eab807325ee58d69c08d66461357d0f961aee383acc3247ed6419706").unwrap(),
        message_encoded: MessageEncoded::new("a466616374696f6e6663726561746566736368656d6178843030343031643736353636373538613562366266633536316631633933366438666338366235623432656132326162316461626634306432343964323764643930363430316664653134376535336634346331303364643032613235343931366265313133653531646531303737613934366133613063313237326239623334383433376776657273696f6e01666669656c6473a26b6465736372697074696f6ea26474797065637374726576616c756571666f7220706c6179696e67206368657373646e616d65a26474797065637374726576616c7565656368657373").unwrap(),
        key_pair: Panda::keypair_from_private("4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176"),
        entry: Panda::entry(&Panda::create_message(MESSAGE_SCHEMA, vec![("name", "chess"), ("description", "for playing chess")]), &Panda::seq_num(1), None, None)
    }
}
