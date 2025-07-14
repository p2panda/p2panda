// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use crate::event::Event;
use crate::group::Group;
use crate::space::Space;
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
pub struct Manager<F, M> {
    forge: F,
    _phantom: PhantomData<M>,
}

impl<F, M> Manager<F, M>
where
    F: Forge<M>,
{
    pub fn space(&self) -> Space<F, M> {
        todo!()
    }

    pub fn create_space(&mut self) -> Space<F, M> {
        todo!()
    }

    pub fn group(&self) -> Group {
        todo!()
    }

    pub fn create_group(&mut self) -> Group {
        todo!()
    }

    pub fn receive(&mut self, _message: &M) -> Vec<Event<F, M>> {
        todo!()
    }
}
