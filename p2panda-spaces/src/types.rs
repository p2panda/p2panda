// SPDX-License-Identifier: MIT OR Apache-2.0

//! Types used across p2panda-spaces.

use p2panda_auth::group::GroupCrdtInnerState as AuthInnerState;
use p2panda_auth::traits::{Conditions, Resolver};
use p2panda_core::hash::Hash;
use p2panda_core::identity::VerifyingKey;
use p2panda_encryption::key_manager::KeyManager;
use p2panda_encryption::key_registry::KeyRegistry;

use crate::auth::message::AuthMessage;
use crate::encryption::dgm::EncryptionGroupMembership;
use crate::encryption::orderer::EncryptionOrderer;

// ~~~ Auth ~~~

pub type AuthGroup<C, RS> =
    p2panda_auth::group::GroupCrdt<VerifyingKey, Hash, AuthMessage<C>, C, RS>;

pub type AuthGroupState<C> =
    p2panda_auth::group::GroupCrdtState<VerifyingKey, Hash, AuthMessage<C>, C>;

pub type AuthGroupError<C, RS> =
    p2panda_auth::group::GroupCrdtError<VerifyingKey, Hash, AuthMessage<C>, C, RS>;

pub type AuthGroupAction<C> = p2panda_auth::group::GroupAction<VerifyingKey, C>;

pub type StrongRemoveResolver<C> =
    p2panda_auth::group::resolver::StrongRemove<VerifyingKey, Hash, AuthMessage<C>, C>;

pub trait AuthResolver<C>:
    Resolver<
        VerifyingKey,
        Hash,
        AuthMessage<C>,
        C,
        State = AuthInnerState<VerifyingKey, Hash, AuthMessage<C>, C>,
    >
{
}

// Required as we define a new super-trait above with non-generic actor id,
// operation id and message type.
impl<C> AuthResolver<C> for StrongRemoveResolver<C> where C: Conditions {}

// ~~~ Encryption ~~~

pub type EncryptionGroup = p2panda_encryption::data_scheme::EncryptionGroup<
    VerifyingKey,
    Hash,
    KeyRegistry<VerifyingKey>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer,
>;

pub type EncryptionGroupState = p2panda_encryption::data_scheme::GroupState<
    VerifyingKey,
    Hash,
    KeyRegistry<VerifyingKey>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer,
>;

pub type EncryptionDirectMessage =
    p2panda_encryption::data_scheme::DirectMessage<VerifyingKey, Hash, EncryptionGroupMembership>;

pub type EncryptionControlMessage = p2panda_encryption::data_scheme::ControlMessage<VerifyingKey>;

pub type EncryptionGroupError = p2panda_encryption::data_scheme::GroupError<
    VerifyingKey,
    Hash,
    KeyRegistry<VerifyingKey>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer,
>;

pub type EncryptionGroupOutput = p2panda_encryption::data_scheme::GroupOutput<
    VerifyingKey,
    Hash,
    EncryptionGroupMembership,
    EncryptionOrderer,
>;
