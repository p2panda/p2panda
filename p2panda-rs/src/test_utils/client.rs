// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mock p2panda client.
//! 
//! This client mocks functionality which would be implemented in a real world p2panda client. 
//! It does so in a simplistic manner and should only be used in a testing environment or demo 
//! environment.

use crate::entry::{sign_and_encode, Entry, EntrySigned};
use crate::identity::{Author, KeyPair};
use crate::message::Message;

use crate::test_utils::utils::NextEntryArgs;

/// A helper struct which represents a client in the pandaverse. It doesn't do much except wrap an Author identity
/// and it's KeyPair and create and sign entries.
#[derive(Debug)]
pub struct Client {
    /// Name of this client
    pub name: String,
    /// The keypair owned by this client
    pub key_pair: KeyPair,
}

impl Client {
    /// Create a new client passing in name and key_pair
    pub fn new(name: String, key_pair: KeyPair) -> Self {
        Self { name, key_pair }
    }

    /// Get an author instance for this client
    pub fn author(&self) -> Author {
        Author::new(&self.public_key()).unwrap()
    }

    /// Get the private key for this author
    pub fn private_key(&self) -> String {
        hex::encode(self.key_pair.private_key())
    }

    /// Get the public key identifier for this author
    pub fn public_key(&self) -> String {
        hex::encode(self.key_pair.public_key())
    }

    /// Get the name of this author
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

        sign_and_encode(&entry, &self.key_pair).unwrap()
    }
}
