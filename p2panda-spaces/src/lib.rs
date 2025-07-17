// SPDX-License-Identifier: MIT OR Apache-2.0

// @TODO: Remove this later.
#![allow(unused)]

use std::convert::Infallible;
use std::fmt::{Debug, Display, Formatter};

use p2panda_auth::traits::IdentityHandle as AuthIdentityHandle;
use p2panda_auth::traits::OperationId as AuthOperationId;
use p2panda_core::{Hash, PublicKey};
use p2panda_encryption::traits::IdentityHandle as EncryptionIdentityHandle;
use p2panda_encryption::traits::OperationId as EncryptionOperationId;
use serde::{Deserialize, Serialize};

use crate::dgm::EncryptionGroupMembership;
use crate::key_manager::KeyManager;
use crate::key_registry::KeyRegistry;
use crate::orderer::{AuthOrderer, EncryptionOrderer};

mod dgm;
mod event;
mod group;
mod key_manager;
mod key_registry;
mod manager;
mod orderer;
mod space;
mod store;
pub mod traits;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActorId(pub(crate) PublicKey);

impl AuthIdentityHandle for ActorId {}
impl EncryptionIdentityHandle for ActorId {}

impl Display for ActorId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl From<PublicKey> for ActorId {
    fn from(public_key: PublicKey) -> Self {
        Self(public_key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OperationId(pub(crate) Hash);

impl Display for OperationId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl AuthOperationId for OperationId {}
impl EncryptionOperationId for OperationId {}

type AuthGroupError<C, RS> =
    p2panda_auth::group::GroupCrdtError<ActorId, OperationId, C, RS, AuthOrderer, AuthDummyStore>;

type AuthControlMessage<C> = p2panda_auth::group::GroupControlMessage<ActorId, C>;

type AuthGroupState<C, RS> =
    p2panda_auth::group::GroupCrdtState<ActorId, OperationId, C, RS, AuthOrderer, AuthDummyStore>;

// @TODO: Will change in `p2panda-auth`.
#[derive(Debug, Clone)]
struct AuthDummyStore;

impl<C, RS> p2panda_auth::traits::GroupStore<ActorId, OperationId, C, RS, AuthOrderer>
    for AuthDummyStore
where
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

pub trait Conditions: Clone + Debug + PartialEq + PartialOrd {}

type EncryptionGroup = p2panda_encryption::data_scheme::EncryptionGroup<
    ActorId,
    OperationId,
    KeyRegistry,
    EncryptionGroupMembership,
    KeyManager,
    EncryptionOrderer,
>;

type EncryptionDirectMessage =
    p2panda_encryption::data_scheme::DirectMessage<ActorId, OperationId, EncryptionGroupMembership>;

type EncryptionControlMessage = p2panda_encryption::data_scheme::ControlMessage<ActorId>;
