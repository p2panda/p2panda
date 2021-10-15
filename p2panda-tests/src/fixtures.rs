// SPDX-License-Identifier: AGPL-3.0-or-later
use rstest::fixture;

use p2panda_rs::entry::{Entry, EntrySigned, SeqNum};
use p2panda_rs::hash::Hash;
use p2panda_rs::identity::KeyPair;
use p2panda_rs::message::{Message, MessageEncoded};

use crate::utils::TestPanda;

const CHESS_SCHEMA: &str  = "00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437";

/// `rstest` fixtures which can be injected into tests 
///
/// From the `rstest` docs: "rstest uses procedural macros to help you on writing fixtures and table-based tests.
/// The core idea is that you can inject your test dependencies by passing them as test arguments."
/// 
/// https://github.com/la10736/rstest

#[fixture]
pub fn key_pair() -> KeyPair {
    TestPanda::keypair()
}

#[fixture]
pub fn message(
    #[default(None)] instance_id: Option<Hash>,
    #[default(Some(vec![("message", "Hello!")]))] fields: Option<Vec<(&str, &str)>>,
) -> Message {
    match fields {
        Some(fields) if instance_id.is_none() => TestPanda::create_message(CHESS_SCHEMA, fields),
        Some(_fields) => todo!(), // update_message()
        None => todo!() // delete_message(),
    }
}

#[fixture]
pub fn entry(
    message: Message,
    #[default(SeqNum::new(1).unwrap())] seq_num: SeqNum,
    #[default(None)] backlink: Option<Hash>,
    #[default(None)] skiplink: Option<Hash>,
) -> Entry {
    TestPanda::entry(&message, &seq_num, backlink, skiplink)
}
pub struct PandaTestFixture {
    pub entry: Entry,
    pub entry_signed_encoded: EntrySigned,
    pub key_pair: KeyPair,
    pub message_encoded: MessageEncoded,
}

#[fixture]
pub fn v0_1_0_fixture() -> PandaTestFixture {
    PandaTestFixture {
        entry_signed_encoded: EntrySigned::new("009cdb3a8c0c4b308173d4c3c43a67a6d013444af99acb8be6c52423746d9aa2c10101f60040190c0d1b8a9bbe5d8b94c8226cdb5d9804af3af6a0c5e34c918864370953dbc7100438f1e5cb0f34bd214c595e37fbb0727f86e9f3eccafa9ba13ed8ef77a04ef01463f550ce62f983494d0eb6051c73a5641025f355758006724e5b730f47a4454c5395eab807325ee58d69c08d66461357d0f961aee383acc3247ed6419706").unwrap(),
        message_encoded: MessageEncoded::new("a466616374696f6e6663726561746566736368656d6178843030343031643736353636373538613562366266633536316631633933366438666338366235623432656132326162316461626634306432343964323764643930363430316664653134376535336634346331303364643032613235343931366265313133653531646531303737613934366133613063313237326239623334383433376776657273696f6e01666669656c6473a26b6465736372697074696f6ea26474797065637374726576616c756571666f7220706c6179696e67206368657373646e616d65a26474797065637374726576616c7565656368657373").unwrap(),
        key_pair: TestPanda::keypair_from_private("4c21b14046f284f87f1ea4be4b973664221ad483079a68ed35a6812553b41176"),
        entry: TestPanda::entry(&TestPanda::create_message(CHESS_SCHEMA, vec![("name", "chess"), ("description", "for playing chess")]), &TestPanda::seq_num(1), None, None)
    }
}

