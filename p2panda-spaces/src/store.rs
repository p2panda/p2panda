// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::Rng;
use p2panda_encryption::data_scheme::SecretBundleState;
use p2panda_encryption::data_scheme::dcgka::DcgkaState;
use p2panda_encryption::key_bundle::LongTermKeyBundle;
use p2panda_encryption::key_manager::{
    KeyManager, KeyManagerError, KeyManagerState, PreKeyBundlesState,
};
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryState};
use p2panda_encryption::two_party::TwoPartyState;
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::PreKeyBundlesStore;
use p2panda_store::spaces::SpacesStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::encryption::dgm::EncryptionMembershipState;
use crate::encryption::orderer::EncryptionOrdererState;
use crate::identity::IdentityError;
use crate::manager::StoreError;
use crate::types::{AuthGroupState, EncryptionGroup, EncryptionGroupState};
use crate::{
    ActorId, AuthMessage, Credentials, Forge, GroupId, MemberId, SpaceId, SpacesArgs,
    global_groups_id,
};

pub struct Context<'a, S, F> {
    pub store: &'a S,
    pub forge: &'a F,
    pub credentials: &'a Credentials,
    pub rng: &'a Rng,
}

impl<S, F> Context<'_, S, F> {
    pub fn my_id(&self) -> ActorId {
        self.credentials.verifying_key()
    }

    pub async fn forge<C>(&self, args: SpacesArgs<C>) -> Result<F::Message, IdentityError<F, C>>
    where
        F: Forge<C>,
        C: Conditions,
    {
        // NOTE: Make sure the forge itself is running within the same transaction, otherwise we'll
        // end up with a deadlock caused by nested transactions.
        self.forge.forge(args).await.map_err(IdentityError::Forge)
    }
}

/// Space state, assembled from the persisted space state and the global key manager and -registry.
#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct SpaceState<C> {
    /// Id of this space.
    pub space_id: SpaceId,

    /// Local group id describing space membership.
    pub group_id: GroupId,

    /// Local group graph describing membership changes.
    pub groups_y: AuthGroupState<C>,

    /// Encryption state for this space, including global key-registry and -manager for secrets.
    ///
    /// Key manager and -registry states are global, but when a space is loaded they are moved into
    /// it's encryption state (DCGKA) as they get modified during key agreement.
    pub encryption_y: EncryptionGroupState,

    /// Grow-Only-Set of all individuals who have ever been removed from the space, either via
    /// direct removal, or transitively as a member of a removed group.
    pub removed: HashSet<MemberId>,
}

impl<C> SpaceState<C>
where
    C: Conditions,
{
    /// Initialise empty space state.
    ///
    /// Every space starts with an empty "local" group graph and orderer state.
    pub fn init(
        my_id: MemberId,
        space_id: SpaceId,
        group_id: GroupId,
        key_manager_y: KeyManagerState,
        key_registry_y: KeyRegistryState<MemberId>,
    ) -> Self {
        let groups_y = AuthGroupState::new();

        let encryption_y = {
            let encryption_orderer_y = EncryptionOrdererState::new();
            let dgm = EncryptionMembershipState {
                members: Vec::new(),
            };

            // The global key manager & -registry state is nested inside the encryption-group state
            // for the key-agreement API to work with during processing, however we treat this
            // prekey material always as global and _not_ per-space.
            EncryptionGroup::init(
                my_id,
                key_manager_y,
                key_registry_y,
                dgm,
                encryption_orderer_y,
            )
        };

        Self {
            space_id,
            group_id,
            groups_y,
            encryption_y,
            removed: HashSet::new(),
        }
    }

    /// Assemble space state from persisted state and the key manager and -registry.
    fn from_states(
        space_y: SpacesStoreState<C>,
        key_manager_y: KeyManagerState,
        key_registry_y: KeyRegistryState<MemberId>,
    ) -> Self {
        let space_id = space_y.space_id;
        let group_id = space_y.group_id;
        let removed = space_y.removed.clone();
        let (groups_y, encryption_y) =
            space_y.assemble_encryption_state(key_manager_y, key_registry_y);

        Self {
            space_id,
            group_id,
            groups_y,
            encryption_y,
            removed,
        }
    }

    pub fn key_manager_state(&self) -> &KeyManagerState {
        &self.encryption_y.dcgka.my_keys
    }

    pub fn key_registry_state(&self) -> &KeyRegistryState<MemberId> {
        &self.encryption_y.dcgka.pki
    }
}

/// Modified state which will be written to the store on commit.
///
/// Prekey-bundle states are always treated globally but might arrive here via two different paths,
/// either nested within space state (`StateChanges::Space`) or set directly
/// (`StateChanges::PrekeyBundles`).
#[derive(Debug, Default)]
#[allow(clippy::large_enum_variant)]
pub enum StateChanges<C> {
    #[default]
    Nothing,
    GlobalGroups {
        global_groups_y: AuthGroupState<C>,
    },
    PrekeyBundles {
        key_manager_y: Option<KeyManagerState>,
        key_registry_y: Option<KeyRegistryState<MemberId>>,
    },
    Space {
        global_groups_y: Option<AuthGroupState<C>>,
        space_y: SpaceState<C>,
    },
}

/// Open store transaction for the read -> modify -> write cycle, making sure all spaces changes are
/// atomic.
///
/// Dropping the transaction without committing will automatically roll back all changes.
pub struct StateTransaction<'a, S, F, C>
where
    S: Transaction,
{
    pub ctx: Context<'a, S, F>,
    permit: S::Permit,
    _marker: PhantomData<C>,
}

impl<'a, S, F, C> StateTransaction<'a, S, F, C>
where
    S: SpacesStore<SpacesStoreState<C>>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + PreKeyBundlesStore
        + Transaction,
    F: Forge<C>,
    C: Conditions,
{
    pub async fn begin(
        store: &'a S,
        forge: &'a F,
        credentials: &'a Credentials,
        rng: &'a Rng,
    ) -> Result<Self, StoreError> {
        let permit = store
            .begin()
            .await
            .map_err(|err| StoreError::Transaction(err.to_string()))?;

        Ok(Self {
            ctx: Context {
                store,
                forge,
                credentials,
                rng,
            },
            permit,
            _marker: PhantomData,
        })
    }

    /// Read global groups state or initialise it on first use.
    pub async fn global_groups_state(&self) -> Result<AuthGroupState<C>, StoreError> {
        let y = self
            .ctx
            .store
            .get_groups_state_tx(global_groups_id())
            .await
            .map_err(|err| StoreError::GroupsStore(err.to_string()))?;
        Ok(y.unwrap_or_default())
    }

    /// Read global key manager state (prekey secrets) or initialise it on first use.
    pub async fn key_manager_state(&self) -> Result<KeyManagerState, StateError> {
        let y = self
            .ctx
            .store
            .get_prekey_secrets_tx()
            .await
            .map_err(|err| StoreError::KeySecretStore(err.to_string()))?;
        Ok(init_key_manager(self.ctx.credentials, y)?)
    }

    /// Read global key registry state (PKI) or initialise it on first use.
    pub async fn key_registry_state(&self) -> Result<KeyRegistryState<MemberId>, StoreError> {
        let y = self
            .ctx
            .store
            .get_key_registry_tx()
            .await
            .map_err(|err| StoreError::KeyRegistryStore(err.to_string()))?;
        Ok(y.unwrap_or_else(KeyRegistry::init))
    }

    /// Read space state, returns `None` if space does not exist.
    pub async fn space_state(
        &self,
        space_id: &SpaceId,
    ) -> Result<Option<SpaceState<C>>, StateError> {
        let Some(space_y) = self
            .ctx
            .store
            .get_space_state_tx(space_id)
            .await
            .map_err(|err| StoreError::SpacesStore(err.to_string()))?
        else {
            return Ok(None);
        };

        let key_manager_y = self.key_manager_state().await?;
        let key_registry_y = self.key_registry_state().await?;

        Ok(Some(SpaceState::from_states(
            space_y,
            key_manager_y,
            key_registry_y,
        )))
    }

    /// Write changed state to the store and commit the transaction.
    pub async fn commit(self, changes: StateChanges<C>) -> Result<(), StoreError> {
        let store = self.ctx.store;
        let mut no_changes_made = false;

        match changes {
            StateChanges::Nothing => {
                no_changes_made = true;
            }
            StateChanges::GlobalGroups { global_groups_y } => {
                store
                    .set_groups_state_tx(global_groups_id(), &global_groups_y)
                    .await
                    .map_err(|err| StoreError::GroupsStore(err.to_string()))?;
            }
            StateChanges::PrekeyBundles {
                key_manager_y,
                key_registry_y,
            } => {
                if let Some(y) = key_manager_y {
                    store
                        .set_prekey_secrets_tx(y.prekey_bundles())
                        .await
                        .map_err(|err| StoreError::KeySecretStore(err.to_string()))?;
                }

                if let Some(y) = key_registry_y {
                    store
                        .set_key_registry_tx(&y)
                        .await
                        .map_err(|err| StoreError::KeyRegistryStore(err.to_string()))?;
                }
            }
            StateChanges::Space {
                global_groups_y,
                space_y,
            } => {
                if let Some(global_groups_y) = global_groups_y {
                    store
                        .set_groups_state_tx(global_groups_id(), &global_groups_y)
                        .await
                        .map_err(|err| StoreError::GroupsStore(err.to_string()))?;
                }

                let key_manager_y = space_y.key_manager_state();
                let key_registry_y = space_y.key_registry_state();

                store
                    .set_prekey_secrets_tx(key_manager_y.prekey_bundles())
                    .await
                    .map_err(|err| StoreError::KeySecretStore(err.to_string()))?;

                store
                    .set_key_registry_tx(key_registry_y)
                    .await
                    .map_err(|err| StoreError::KeyRegistryStore(err.to_string()))?;

                let space_id = space_y.space_id;
                store
                    .set_space_state_tx(&space_id, &SpacesStoreState::from(space_y))
                    .await
                    .map_err(|err| StoreError::SpacesStore(err.to_string()))?;
            }
        }

        if no_changes_made {
            store
                .rollback(self.permit)
                .await
                .map_err(|err| StoreError::Transaction(err.to_string()))?;
        } else {
            store
                .commit(self.permit)
                .await
                .map_err(|err| StoreError::Transaction(err.to_string()))?;
        }

        Ok(())
    }

    /// Discard all changes and roll back the transaction.
    pub async fn rollback(self) -> Result<(), StoreError> {
        self.ctx
            .store
            .rollback(self.permit)
            .await
            .map_err(|err| StoreError::Transaction(err.to_string()))?;
        Ok(())
    }
}

/// Read-only access to the latest committed state.
///
/// This does not acquire a transaction and thus never blocks or gets blocked by writes.
pub struct StateReader<'a, S, C> {
    store: &'a S,
    credentials: &'a Credentials,
    _marker: PhantomData<C>,
}

impl<'a, S, C> StateReader<'a, S, C>
where
    S: SpacesStore<SpacesStoreState<C>>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + PreKeyBundlesStore,
    C: Conditions,
{
    pub fn new(store: &'a S, credentials: &'a Credentials) -> Self {
        Self {
            store,
            credentials,
            _marker: PhantomData,
        }
    }

    /// Read global groups state, returns empty state if it doesn't exist yet.
    pub async fn global_groups_state(&self) -> Result<AuthGroupState<C>, StoreError> {
        let y = self
            .store
            .get_groups_state(global_groups_id())
            .await
            .map_err(|err| StoreError::GroupsStore(err.to_string()))?;
        Ok(y.unwrap_or_default())
    }

    /// Read key manager state, returns empty state if it doesn't exist yet.
    pub async fn key_manager_state(&self) -> Result<KeyManagerState, StateError> {
        let y = self
            .store
            .get_prekey_secrets()
            .await
            .map_err(|err| StoreError::KeySecretStore(err.to_string()))?;
        Ok(init_key_manager(self.credentials, y)?)
    }

    /// Read key registry state, returns empty state if it doesn't exist yet.
    pub async fn key_registry_state(&self) -> Result<KeyRegistryState<MemberId>, StoreError> {
        let y = self
            .store
            .get_key_registry()
            .await
            .map_err(|err| StoreError::KeyRegistryStore(err.to_string()))?;
        Ok(y.unwrap_or_else(KeyRegistry::init))
    }

    /// Read space state, returns `None` if space does not exist.
    pub async fn space_state(
        &self,
        space_id: &SpaceId,
    ) -> Result<Option<SpaceState<C>>, StateError> {
        let Some(space_y) = self
            .store
            .get_space_state(space_id)
            .await
            .map_err(|err| StoreError::SpacesStore(err.to_string()))?
        else {
            return Ok(None);
        };

        let key_manager_y = self.key_manager_state().await?;
        let key_registry_y = self.key_registry_state().await?;

        Ok(Some(SpaceState::from_states(
            space_y,
            key_manager_y,
            key_registry_y,
        )))
    }
}

/// Assemble key manager state from persisted pre-key bundles (initialised empty on first use) and
/// our identity secret.
fn init_key_manager(
    credentials: &Credentials,
    prekey_bundles_y: Option<PreKeyBundlesState>,
) -> Result<KeyManagerState, KeyManagerError> {
    KeyManager::init_from_prekey_bundles(
        &credentials.identity_secret(),
        prekey_bundles_y.unwrap_or_default(),
    )
}

#[derive(Debug, Error)]
pub enum StateError {
    #[error(transparent)]
    Store(#[from] StoreError),

    #[error("initialisation of key manager failed with: {0}")]
    KeyManager(#[from] KeyManagerError),
}

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
    // TODO: Can we store this in the groups table instead with the given group_id?
    pub groups_y: AuthGroupState<C>,
    pub is_welcomed: bool,
    pub secrets: SecretBundleState,
    // TODO: Verify if this really required here? Maybe ordering is handled on another layer.
    pub orderer: EncryptionOrdererState,
    pub two_party: HashMap<MemberId, TwoPartyState<LongTermKeyBundle>>,
    pub removed: HashSet<MemberId>,
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

impl<C> From<SpaceState<C>> for SpacesStoreState<C>
where
    C: Conditions,
{
    fn from(y: SpaceState<C>) -> Self {
        Self {
            my_id: y.encryption_y.my_id,
            space_id: y.space_id,
            group_id: y.group_id,
            groups_y: y.groups_y,
            is_welcomed: y.encryption_y.is_welcomed,
            secrets: y.encryption_y.secrets,
            orderer: y.encryption_y.orderer,
            two_party: y.encryption_y.dcgka.two_party,
            removed: y.removed,
        }
    }
}
