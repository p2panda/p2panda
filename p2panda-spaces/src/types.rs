// SPDX-License-Identifier: MIT OR Apache-2.0

//! Types used across p2panda-spaces.

use p2panda_auth::group::GroupCrdtInnerState as AuthInnerState;
use p2panda_auth::traits::{Conditions, Resolver};
use p2panda_encryption::key_manager::KeyManager;
use p2panda_encryption::key_registry::KeyRegistry;

use crate::auth::message::AuthMessage;
use crate::encryption::dgm::EncryptionGroupMembership;
use crate::encryption::orderer::EncryptionOrderer;
use crate::{ActorId, MemberId, OperationId};

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
    MemberId,
    OperationId,
    KeyRegistry<MemberId>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer,
>;

pub type EncryptionGroupState = p2panda_encryption::data_scheme::GroupState<
    MemberId,
    OperationId,
    KeyRegistry<MemberId>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer,
>;

pub type EncryptionDirectMessage = p2panda_encryption::data_scheme::DirectMessage<
    MemberId,
    OperationId,
    EncryptionGroupMembership,
>;

pub type EncryptionControlMessage = p2panda_encryption::data_scheme::ControlMessage<MemberId>;

pub type EncryptionGroupError = p2panda_encryption::data_scheme::GroupError<
    MemberId,
    OperationId,
    KeyRegistry<MemberId>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer,
>;

pub type EncryptionGroupOutput = p2panda_encryption::data_scheme::GroupOutput<
    MemberId,
    OperationId,
    EncryptionGroupMembership,
    EncryptionOrderer,
>;
