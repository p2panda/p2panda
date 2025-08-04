// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_encryption::key_bundle::{KeyBundleError, LongTermKeyBundle};
use p2panda_encryption::traits::KeyBundle;

use crate::types::ActorId;

#[derive(Debug)]
pub struct Member {
    id: ActorId,
    key_bundle: LongTermKeyBundle,
}

impl Member {
    pub fn new(id: ActorId, key_bundle: LongTermKeyBundle) -> Self {
        Self { id, key_bundle }
    }

    pub fn id(&self) -> ActorId {
        self.id
    }

    pub fn key_bundle(&self) -> &LongTermKeyBundle {
        &self.key_bundle
    }

    pub fn verify(&self) -> Result<(), KeyBundleError> {
        self.key_bundle.verify()
    }
}
