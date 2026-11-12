// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use p2panda_core::traits::{Digest, Provenance};
use p2panda_core::{Hash, VerifyingKey};
use p2panda_spaces::SpacesArgs;

use crate::operation::Operation;
use crate::spaces::types::AuthCapabilities;

#[derive(Clone, Debug)]
pub struct SpacesMessage {
    operation: Operation,
    args: SpacesArgs<AuthCapabilities>,
}

impl SpacesMessage {
    pub fn into_operation(self) -> Operation {
        self.operation
    }
}

impl From<Operation> for SpacesMessage {
    fn from(operation: Operation) -> Self {
        let args = operation
            .header
            .extensions
            .spaces_args(operation.body.as_ref())
            .expect("at this point we're only dealing with space extensions");
        Self { operation, args }
    }
}

impl Borrow<SpacesArgs<AuthCapabilities>> for SpacesMessage {
    fn borrow(&self) -> &SpacesArgs<AuthCapabilities> {
        &self.args
    }
}

impl Digest<Hash> for SpacesMessage {
    fn hash(&self) -> Hash {
        self.operation.hash
    }
}

impl Provenance<VerifyingKey> for SpacesMessage {
    fn author(&self) -> VerifyingKey {
        self.operation.author()
    }

    fn verify(&self) -> bool {
        self.operation.verify()
    }
}
