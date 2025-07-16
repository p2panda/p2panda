// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::fmt::Debug;

use p2panda_auth::traits::IdentityHandle as AuthIdentityHandle;
use p2panda_auth::traits::OperationId as AuthOperationId;
use p2panda_core::{Hash, PublicKey};

use crate::orderer::AuthOrderer;

mod event;
mod group;
mod manager;
mod orderer;
mod space;
mod store;
pub mod traits;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActorId(pub(crate) PublicKey);

impl AuthIdentityHandle for ActorId {}

impl From<PublicKey> for ActorId {
    fn from(public_key: PublicKey) -> Self {
        Self(public_key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperationId(pub(crate) Hash);

impl AuthOperationId for OperationId {}

type AuthControlMessage<C> = p2panda_auth::group::GroupControlMessage<ActorId, C>;

type AuthGroupState<C, RS> =
    p2panda_auth::group::GroupCrdtState<ActorId, OperationId, C, RS, AuthOrderer, AuthDummyStore>;

// @TODO: Will change in `p2panda-auth`.
#[derive(Debug)]
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

    fn get(&self, id: &ActorId) -> Result<Option<AuthGroupState<C, RS>>, Self::Error> {
        todo!()
    }
}

pub trait Conditions: Clone + Debug + PartialEq + PartialOrd {}
