// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::VerifyingKey;

use crate::forge::{ForgeError, OperationForge};
use crate::spaces::SpaceId;
use crate::spaces::message::SpacesMessage;
use crate::spaces::types::AuthCapabilities;

impl p2panda_spaces::traits::Forge<SpaceId, AuthCapabilities> for OperationForge {
    type Message = SpacesMessage;

    type Error = ForgeError;

    fn verifying_key(&self) -> VerifyingKey {
        todo!()
    }

    async fn forge(
        &self,
        _args: p2panda_spaces::SpacesArgs<SpaceId, AuthCapabilities>,
    ) -> Result<SpacesMessage, Self::Error> {
        todo!()
    }
}
