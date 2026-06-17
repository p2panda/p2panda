// SPDX-License-Identifier: MIT OR Apache-2.0

//! Types used across p2panda-spaces.

use p2panda_auth::group::GroupCrdtInnerState as AuthInnerState;
use p2panda_auth::traits::{Conditions, Resolver};
use p2panda_core::hash::Hash;
use p2panda_core::identity::VerifyingKey;
use p2panda_encryption::key_manager::KeyManager;
use p2panda_encryption::key_registry::KeyRegistry;

use crate::SpacesArgs;
use crate::auth::message::AuthMessage;
use crate::encryption::dgm::EncryptionGroupMembership;
use crate::encryption::orderer::EncryptionOrderer;
use crate::space::SpaceState;

pub type ActorId = VerifyingKey;

pub type OperationId = Hash;

// ~~~ Auth ~~~

pub type AuthGroup<C, RS> =
    p2panda_auth::group::GroupCrdt<ActorId, OperationId, AuthMessage<C>, C, RS>;

pub type AuthGroupState<C> =
    p2panda_auth::group::GroupCrdtState<ActorId, OperationId, AuthMessage<C>, C>;

pub type AuthGroupError<C, RS> =
    p2panda_auth::group::GroupCrdtError<ActorId, OperationId, AuthMessage<C>, C, RS>;

pub type AuthGroupAction<C> = p2panda_auth::group::GroupAction<ActorId, C>;

pub type StrongRemoveResolver<C> =
    p2panda_auth::group::resolver::StrongRemove<ActorId, OperationId, AuthMessage<C>, C>;

pub trait AuthResolver<C>:
    Resolver<
        ActorId,
        OperationId,
        AuthMessage<C>,
        C,
        State = AuthInnerState<ActorId, OperationId, AuthMessage<C>, C>,
    >
{
}

// Required as we define a new super-trait above with non-generic actor id,
// operation id and message type.
impl<C> AuthResolver<C> for StrongRemoveResolver<C> where C: Conditions {}

// ~~~ Encryption ~~~

pub type EncryptionGroup = p2panda_encryption::data_scheme::EncryptionGroup<
    ActorId,
    OperationId,
    KeyRegistry<ActorId>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer,
>;

pub type EncryptionGroupState = p2panda_encryption::data_scheme::GroupState<
    ActorId,
    OperationId,
    KeyRegistry<ActorId>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer,
>;

pub type EncryptionDirectMessage =
    p2panda_encryption::data_scheme::DirectMessage<ActorId, OperationId, EncryptionGroupMembership>;

pub type EncryptionControlMessage = p2panda_encryption::data_scheme::ControlMessage<ActorId>;

pub type EncryptionGroupError = p2panda_encryption::data_scheme::GroupError<
    ActorId,
    OperationId,
    KeyRegistry<ActorId>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer,
>;

pub type EncryptionGroupOutput = p2panda_encryption::data_scheme::GroupOutput<
    ActorId,
    OperationId,
    EncryptionGroupMembership,
    EncryptionOrderer,
>;

// ~~~ Stores ~~~

pub type SpacesMessage<ID, C> = p2panda_store::spaces::SpacesMessage<SpacesArgs<ID, C>>;
pub trait SpacesStore<ID, C>: p2panda_store::spaces::SpacesStore<ID, SpaceState<ID, C>> {}
pub trait SpacesStoreWrite<ID, C>:
    p2panda_store::spaces::SpacesStoreWrite<ID, SpaceState<ID, C>>
{
}
pub trait SpacesMessageStore<ID, C>:
    p2panda_store::spaces::SpacesMessageStore<OperationId, SpacesArgs<ID, C>>
{
}

pub type GroupsContextID = Hash;

pub trait GroupsStore<C>:
    p2panda_store::groups::GroupsStore<GroupsContextID, AuthGroupState<C>>
{
}

impl<ID, C, T> SpacesStore<ID, C> for T where
    T: p2panda_store::spaces::SpacesStore<ID, SpaceState<ID, C>>
{
}

impl<ID, C, T> SpacesStoreWrite<ID, C> for T where
    T: p2panda_store::spaces::SpacesStoreWrite<ID, SpaceState<ID, C>>
{
}

impl<ID, C, T> SpacesMessageStore<ID, C> for T where
    T: p2panda_store::spaces::SpacesMessageStore<OperationId, SpacesArgs<ID, C>>
{
}

impl<C, T> GroupsStore<C> for T where
    T: p2panda_store::groups::GroupsStore<GroupsContextID, AuthGroupState<C>>
{
}
