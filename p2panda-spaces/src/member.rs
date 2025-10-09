// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_encryption::key_bundle::{KeyBundleError, LongTermKeyBundle};
use p2panda_encryption::traits::KeyBundle;

use crate::types::ActorId;

// @NOTE(adz) **Security:** This struct does _not_ guarantee if the member's handle / id is
// authentic. We or applications will need to provide an authentication scheme and validate
// `Member` before using it anywhere to prevent impersonation attacks.
//
// Since we're currently not allowing to construct `Member` from "the outside" (all instances are
// provided by our API which derived everything from signed messages) I don't see an issue yet, but
// care will be required as soon as `Member` becomes serializable etc.
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
