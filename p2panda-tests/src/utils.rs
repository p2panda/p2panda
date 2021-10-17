use p2panda_rs::entry::{Entry, EntrySigned};
use p2panda_rs::message::MessageEncoded;
use p2panda_rs::identity::KeyPair;

pub struct Fixture {
    pub entry: Entry,
    pub entry_signed_encoded: EntrySigned,
    pub key_pair: KeyPair,
    pub message_encoded: MessageEncoded,
}

pub const CHESS_SCHEMA: &str  = "00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437";
