// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_rs::entry::{sign_and_encode, Entry, EntrySigned};
use p2panda_rs::identity::{Author, KeyPair};
use p2panda_rs::message::Message;

use crate::utils::NextEntryArgs;

/// A helper struct which represents a client in the pandaverse. It doesn't do much except wrap an Author identity
/// and it's KeyPair and create and sign entries.
#[derive(Debug)]
pub struct Client {
    pub name: String,
    pub key_pair: KeyPair,
}

impl Client {
    pub fn new(name: String, key_pair: KeyPair) -> Self {
        Self { name, key_pair }
    }

    pub fn author(&self) -> Author {
        Author::new(&self.public_key()).unwrap()
    }

    pub fn private_key(&self) -> String {
        hex::encode(self.key_pair.private_key())
    }

    pub fn public_key(&self) -> String {
        hex::encode(self.key_pair.public_key())
    }

    pub fn name(&self) -> String {
        self.name.to_owned()
    }

    /// Create, sign and encode an entry
    pub fn signed_encoded_entry(&self, message: Message, entry_args: NextEntryArgs) -> EntrySigned {
        // Construct entry from message and entry args then sign and encode it
        let entry = Entry::new(
            &entry_args.log_id,
            Some(&message),
            entry_args.skiplink.as_ref(),
            entry_args.backlink.as_ref(),
            &entry_args.seq_num,
        )
        .unwrap();

        let entry_encoded = sign_and_encode(&entry, &self.key_pair).unwrap();

        entry_encoded
    }
}
