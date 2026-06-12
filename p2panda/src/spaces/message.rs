// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use p2panda_spaces::traits::AuthoredMessage;
use p2panda_spaces::{ActorId, SpacesArgs};

use crate::operation::{ExtensionsVariantV1, Operation};
use crate::spaces::SpaceId;
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

impl Borrow<SpacesArgs<SpaceId, AuthCapabilities>> for SpacesMessage {
    fn borrow(&self) -> &SpacesArgs<SpaceId, AuthCapabilities> {
        match &self.0.header.extensions.variant {
            ExtensionsVariantV1::Space(extensions) => &extensions.args,
            _ => unreachable!("at this point we're only dealing with space extensions"),
        }
    }
}

impl AuthoredMessage for SpacesMessage {
    fn id(&self) -> p2panda_spaces::OperationId {
        self.0.hash.into()
    }

    fn author(&self) -> ActorId {
        self.0.header.verifying_key.into()
    }
}
