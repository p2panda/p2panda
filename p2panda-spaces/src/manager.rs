// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use crate::event::Event;
use crate::group::Group;
use crate::space::Space;
use crate::store::SpacesStore;
use crate::traits::Forge;

/// Create and manage spaces and groups.
///
/// Takes care of ingesting operations, updating spaces, groups and member key-material. Has access
/// to the operation and group stores, orderer, key-registry and key-manager.
///
/// Routes operations to the correct space(s), group(s) or member.
///
/// Only one instance of `Spaces` per app user.
///
/// Operations are created and published within the spaces service, reacting to arriving
/// operations, due to api calls (create group, create space), or triggered by key-bundles
/// expiring.
///
/// Users of spaces can subscribe to events which inform about member, group or space state
/// changes, application data being decrypted, pre-key bundles being published, we were added or
/// removed from a space.
///
/// Is agnostic to current p2panda-streams, networking layer, data type?
pub struct Manager<S, F, M> {
    forge: F,
    store: S,
    _marker: PhantomData<M>,
}

impl<S, F, M> Manager<S, F, M>
where
    S: SpacesStore,
    F: Forge<M>,
{
    pub fn new(store: S, forge: F) -> Self {
        Self {
            forge,
            store,
            _marker: PhantomData,
        }
    }

    pub fn space(&self) -> Space<S, F, M> {
        todo!()
    }

    pub fn create_space(&mut self) -> Space<S, F, M> {
        todo!()
    }

    pub fn group(&self) -> Group {
        todo!()
    }

    pub fn create_group(&mut self) -> Group {
        todo!()
    }

    pub fn process(&mut self, _message: &M) -> Vec<Event<S, F, M>> {
        todo!()
    }
}
