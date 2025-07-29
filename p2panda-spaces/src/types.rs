// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;
use std::sync::LazyLock;

use p2panda_auth::traits::{IdentityHandle as AuthIdentityHandle, OperationId as AuthOperationId};
use p2panda_core::hash::{HASH_LEN, Hash};
use p2panda_core::identity::{PUBLIC_KEY_LEN, PublicKey};
use p2panda_core::{HashError, IdentityError};
use p2panda_encryption::key_manager::KeyManager;
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_encryption::traits::{
    IdentityHandle as EncryptionIdentityHandle, OperationId as EncryptionOperationId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::auth::orderer::AuthOrderer;
use crate::encryption::dgm::EncryptionGroupMembership;
use crate::encryption::orderer::EncryptionOrderer;

pub const ACTOR_ID_SIZE: usize = PUBLIC_KEY_LEN;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ActorId(pub(crate) PublicKey);

impl AuthIdentityHandle for ActorId {}

impl EncryptionIdentityHandle for ActorId {}

impl ActorId {
    pub fn from_bytes(bytes: &[u8; ACTOR_ID_SIZE]) -> Result<Self, ActorIdError> {
        Ok(Self(PublicKey::from_bytes(bytes)?))
    }

    pub fn as_bytes(&self) -> &[u8; ACTOR_ID_SIZE] {
        self.0.as_bytes()
    }

    pub fn to_hex(&self) -> String {
        self.0.to_hex().to_string()
    }

    // When processing locally created operations we handle unsigned messages where the actor id is
    // not known and not required. In these cases we need to satisfy the trait interfaces using a
    // placeholder value.
    pub(crate) fn placeholder() -> Self {
        static PLACEHOLDER_PUBLIC_KEY: LazyLock<PublicKey> = LazyLock::new(|| {
            PublicKey::from_bytes(&[0; PUBLIC_KEY_LEN])
                .expect("can create public key from constant bytes")
        });
        Self(*PLACEHOLDER_PUBLIC_KEY)
    }
}

impl Display for ActorId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<PublicKey> for ActorId {
    fn from(public_key: PublicKey) -> Self {
        Self(public_key)
    }
}

impl TryFrom<[u8; ACTOR_ID_SIZE]> for ActorId {
    type Error = ActorIdError;

    fn try_from(bytes: [u8; ACTOR_ID_SIZE]) -> Result<Self, Self::Error> {
        Ok(Self(PublicKey::from_bytes(&bytes)?))
    }
}

impl TryFrom<&[u8]> for ActorId {
    type Error = ActorIdError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self(PublicKey::try_from(bytes)?))
    }
}

impl TryFrom<ActorId> for PublicKey {
    type Error = ActorIdError;

    fn try_from(actor_id: ActorId) -> Result<Self, Self::Error> {
        Ok(PublicKey::from_bytes(actor_id.as_bytes())?)
    }
}

impl FromStr for ActorId {
    type Err = ActorIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(PublicKey::from_str(value)?))
    }
}

#[derive(Debug, Error)]
pub enum ActorIdError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
}

pub const OPERATION_ID_SIZE: usize = HASH_LEN;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OperationId(pub(crate) Hash);

impl AuthOperationId for OperationId {}

impl EncryptionOperationId for OperationId {}

impl OperationId {
    pub fn as_bytes(&self) -> &[u8; OPERATION_ID_SIZE] {
        self.0.as_bytes()
    }

    pub fn to_hex(&self) -> String {
        self.0.to_hex().to_string()
    }

    // When processing locally created operations we handle unsigned messages where the operation
    // id is not known and not required. In these cases we need to satisfy the trait interfaces
    // using a placeholder value.
    pub(crate) fn placeholder() -> Self {
        static PLACEHOLDER_ID: Hash = Hash::from_bytes([0; HASH_LEN]);
        Self(PLACEHOLDER_ID)
    }
}

impl Display for OperationId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Hash> for OperationId {
    fn from(value: Hash) -> Self {
        Self(value)
    }
}

impl From<[u8; OPERATION_ID_SIZE]> for OperationId {
    fn from(bytes: [u8; OPERATION_ID_SIZE]) -> Self {
        Self(Hash::from_bytes(bytes))
    }
}

impl FromStr for OperationId {
    type Err = OperationIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Hash::from_str(value)?))
    }
}

#[derive(Debug, Error)]
pub enum OperationIdError {
    #[error(transparent)]
    Hash(#[from] HashError),
}

// ~~~ Auth ~~~

pub type AuthGroup<C, RS> =
    p2panda_auth::group::GroupCrdt<ActorId, OperationId, C, RS, AuthOrderer, AuthDummyStore>;

pub type AuthGroupState<C, RS> =
    p2panda_auth::group::GroupCrdtState<ActorId, OperationId, C, RS, AuthOrderer, AuthDummyStore>;

pub type AuthGroupError<C, RS> =
    p2panda_auth::group::GroupCrdtError<ActorId, OperationId, C, RS, AuthOrderer, AuthDummyStore>;

pub type AuthControlMessage<C> = p2panda_auth::group::GroupControlMessage<ActorId, C>;

pub type AuthGroupAction<C> = p2panda_auth::group::GroupAction<ActorId, C>;

pub type StrongRemoveResolver<C> = p2panda_auth::group::resolver::StrongRemove<
    ActorId,
    OperationId,
    C,
    AuthOrderer,
    AuthDummyStore,
>;

// ~~~ Encryption ~~~

pub type EncryptionGroup<M> = p2panda_encryption::data_scheme::EncryptionGroup<
    ActorId,
    OperationId,
    KeyRegistry<ActorId>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer<M>,
>;

pub type EncryptionGroupState<M> = p2panda_encryption::data_scheme::GroupState<
    ActorId,
    OperationId,
    KeyRegistry<ActorId>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer<M>,
>;

pub type EncryptionDirectMessage =
    p2panda_encryption::data_scheme::DirectMessage<ActorId, OperationId, EncryptionGroupMembership>;

pub type EncryptionControlMessage = p2panda_encryption::data_scheme::ControlMessage<ActorId>;

pub type EncryptionGroupError<M> = p2panda_encryption::data_scheme::GroupError<
    ActorId,
    OperationId,
    KeyRegistry<ActorId>,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer<M>,
>;

pub type EncryptionGroupOutput<M> = p2panda_encryption::data_scheme::GroupOutput<
    ActorId,
    OperationId,
    EncryptionGroupMembership,
    EncryptionOrderer<M>,
>;

// ~~~ Hacks ~~~

// @TODO: Will change in `p2panda-auth`.
#[derive(Debug, Clone)]
pub struct AuthDummyStore;

impl<C, RS> p2panda_auth::traits::GroupStore<ActorId, OperationId, C, RS, AuthOrderer>
    for AuthDummyStore
where
    C: Conditions,
    Self: Sized,
{
    type Error = Infallible;

    fn insert(&self, _id: &ActorId, _group: &AuthGroupState<C, RS>) -> Result<(), Self::Error> {
        // Noop: Writing new group state will be handled outside of this layer.
        Ok(())
    }

    fn get(&self, _id: &ActorId) -> Result<Option<AuthGroupState<C, RS>>, Self::Error> {
        todo!()
    }
}

// @TODO: this supertrait should be defined in p2panda-auth
pub trait Conditions: Clone + Debug + PartialEq + PartialOrd {}
