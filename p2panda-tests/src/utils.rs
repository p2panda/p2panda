use serde::Serialize;

use p2panda_rs::entry::{Entry, EntrySigned, LogId, SeqNum};
use p2panda_rs::identity::KeyPair;
use p2panda_rs::message::MessageEncoded;
use p2panda_rs::hash::Hash;

pub struct Fixture {
    pub entry: Entry,
    pub entry_signed_encoded: EntrySigned,
    pub key_pair: KeyPair,
    pub message_encoded: MessageEncoded,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct NextEntryArgs {
    pub entryHashBacklink: Option<Hash>,
    pub entryHashSkiplink: Option<Hash>,
    pub seqNum: SeqNum,
    pub logId: LogId,
}

pub const MESSAGE_SCHEMA: &str  = "00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437";

pub const GENERIC_HASH: &str  = "0040cf94f6d605657e90c543b0c919070cdaaf7209c5e1ea58acb8f3568fa2114268dc9ac3bafe12af277d286fce7dc59b7c0c348973c4e9dacbe79485e56ac2a702";

