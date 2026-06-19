// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use p2panda_encryption::data_scheme::SecretBundleState;
use p2panda_encryption::data_scheme::dcgka::DcgkaState;
use p2panda_encryption::key_bundle::LongTermKeyBundle;
use p2panda_encryption::key_manager::KeyManagerState;
use p2panda_encryption::key_registry::KeyRegistryState;
use p2panda_encryption::two_party::TwoPartyState;
use serde::{Deserialize, Serialize};

use crate::encryption::dgm::EncryptionMembershipState;
use crate::encryption::orderer::EncryptionOrdererState;
use crate::space::SpacesState;
use crate::types::{AuthGroupState, EncryptionGroupState};
use crate::{GroupId, MemberId, SpaceId};

/// Spaces state which is encoded and persisted in database.
//
// TODO: Ideally this moves into p2panda-store eventually as the representation / encoding is only a
// concern for the database (serde).
//
// TODO: Introduce versioning and more efficient encoding.
#[derive(Debug, Serialize, Deserialize)]
pub struct SpacesStoreState<C> {
    pub my_id: MemberId,
    pub space_id: SpaceId,
    pub group_id: GroupId,
    // TODO: Check if all fields in this state need to be persisted.
    pub groups_y: AuthGroupState<C>,
    pub is_welcomed: bool,
    pub secrets: SecretBundleState,
    // TODO: Verify if this really required here? Maybe ordering is handled on another layer.
    pub orderer: EncryptionOrdererState,
    pub two_party: HashMap<MemberId, TwoPartyState<LongTermKeyBundle>>,
}

impl<C> SpacesStoreState<C> {
    pub fn assemble_encryption_state(
        self,
        my_keys: KeyManagerState,
        pki: KeyRegistryState<MemberId>,
    ) -> (AuthGroupState<C>, EncryptionGroupState) {
        let groups_y = self.groups_y;

        let encryption_y = EncryptionGroupState {
            my_id: self.my_id,
            dcgka: DcgkaState {
                // Inject latest pre-key material to DCGKA state.
                pki,
                my_keys,
                my_id: self.my_id,
                two_party: self.two_party,
                // The DGM is a no-op here, so we can just always use the default.
                dgm: EncryptionMembershipState::default(),
            },
            orderer: self.orderer,
            secrets: self.secrets,
            is_welcomed: self.is_welcomed,
        };

        (groups_y, encryption_y)
    }
}

impl<C> From<SpacesState<C>> for SpacesStoreState<C> {
    fn from(y: SpacesState<C>) -> Self {
        Self {
            my_id: y.encryption_y.my_id,
            space_id: y.space_id,
            group_id: y.group_id,
            groups_y: y.groups_y,
            is_welcomed: y.encryption_y.is_welcomed,
            secrets: y.encryption_y.secrets,
            orderer: y.encryption_y.orderer,
            two_party: y.encryption_y.dcgka.two_party,
        }
    }
}
