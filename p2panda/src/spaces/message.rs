// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use p2panda_core::traits::{Digest, Provenance};
use p2panda_core::{Hash, VerifyingKey};
use p2panda_spaces::SpacesArgs;

use crate::operation::{ExtensionsVariantV1, Operation};
use crate::spaces::types::AuthCapabilities;

#[derive(Clone, Debug)]
pub struct SpacesMessage(Operation);

impl SpacesMessage {
    pub fn into_operation(self) -> Operation {
        self.0
    }
}

impl From<Operation> for SpacesMessage {
    fn from(operation: Operation) -> Self {
        Self(operation)
    }
}

impl Borrow<SpacesArgs<AuthCapabilities>> for SpacesMessage {
    fn borrow(&self) -> &SpacesArgs<AuthCapabilities> {
        match &self.0.header.extensions.variant {
            ExtensionsVariantV1::Space(extensions) => &extensions.args,
            _ => unreachable!("at this point we're only dealing with space extensions"),
        }
    }
}

impl Digest<Hash> for SpacesMessage {
    fn hash(&self) -> Hash {
        self.0.hash
    }
}

impl Provenance<VerifyingKey> for SpacesMessage {
    fn author(&self) -> VerifyingKey {
        self.0.author()
    }

    fn verify(&self) -> bool {
        self.0.verify()
    }
}
