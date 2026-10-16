// SPDX-License-Identifier: MIT OR Apache-2.0

//! Handles to await spaces, groups and members which might not be created or joined yet.
use p2panda_auth::traits::Conditions;
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::PreKeyBundlesStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore};

use crate::event::SpaceEvent;
use crate::manager::{Manager, ManagerError};
use crate::space::Space;
use crate::utils::is_secret_member;
use crate::{AuthMessage, Forge, SpaceId, SpacesStoreState};

#[derive(Debug)]
pub struct PendingSpace<S, F, C> {
    id: SpaceId,
    manager_ref: Manager<S, F, C>,
}

impl<S, F, C> PendingSpace<S, F, C> {
    pub(crate) fn new(manager_ref: Manager<S, F, C>, id: SpaceId) -> Self {
        Self { id, manager_ref }
    }

    pub fn id(&self) -> SpaceId {
        self.id
    }
}

impl<S, F, C> PendingSpace<S, F, C>
where
    S: Clone
        + SpacesStore<SpacesStoreState<C>>
        + SpacesMessageStore<F::Message>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + PreKeyBundlesStore
        + Transaction,
    F: Forge<C>,
    C: Conditions,
{
    /// Wait until the space is known to us.
    ///
    /// Resolves as soon as we processed the "create" message of the space, even if we're not a
    /// member of it. Use `joined` to wait until we can read and publish to the space.
    ///
    /// This never resolves if the space never arrives, consider wrapping it in a timeout.
    pub async fn created(self) -> Result<Space<S, F, C>, ManagerError<F, C>> {
        let id = self.id;

        // Subscribe _before_ checking the store, otherwise we might miss a notification.
        let waiter = self.manager_ref.waiters().spaces.wait_for(
            move |event| matches!(event, SpaceEvent::Created { space_id, .. } if *space_id == id),
        );

        // Space exists already.
        if !self.manager_ref.has_space(id).await? {
            waiter.await;
        }

        Ok(Space::new(self.manager_ref, id))
    }

    /// Wait until we joined the space with more than pull access, allowing us to decrypt
    /// application messages.
    ///
    /// This never resolves if we're never added or been ejected, consider wrapping it in a timeout.
    pub async fn joined(self) -> Result<Space<S, F, C>, ManagerError<F, C>> {
        let id = self.id;
        let me = self.manager_ref.id();

        // Subscribe _before_ checking the store, otherwise we might miss a notification.
        let waiter = self.manager_ref.waiters().spaces.wait_for(move |event| {
            matches!(
                event,
                SpaceEvent::Created { space_id, context, .. }
                    | SpaceEvent::Added { space_id, context, .. }
                    | SpaceEvent::Promoted { space_id, context, .. }
                    if *space_id == id && is_secret_member(&context.members, me)
            )
        });

        // We've joined already.
        if !self.manager_ref.has_joined_space(id).await? {
            waiter.await;
        }

        Ok(Space::new(self.manager_ref, id))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use p2panda_auth::Access;

    use crate::SpaceId;
    use crate::test_utils::TestPeer;

    use super::PendingSpace;

    #[tokio::test]
    async fn created_resolves_immediately() {
        let alice = TestPeer::new(0).await;

        let space_id = SpaceId::digest(b"my space");
        alice.manager.create_space(space_id, &[]).await.unwrap();

        let space = PendingSpace::new(alice.manager.clone(), space_id)
            .created()
            .await
            .unwrap();
        assert_eq!(space.id(), space_id);
    }

    #[tokio::test]
    async fn created_resolves_when_space_arrives() {
        let alice = TestPeer::new(0).await;
        let bob = TestPeer::new(1).await;

        alice
            .manager
            .register_member(&bob.manager.me().await.unwrap())
            .await
            .unwrap();

        // Bob waits for a space which doesn't exist yet.
        let space_id = SpaceId::digest(b"my space");
        let handle = tokio::spawn(PendingSpace::new(bob.manager.clone(), space_id).created());

        // Alice creates the space with Bob and they processes the messages.
        let (_, output) = alice
            .manager
            .create_space(space_id, &[(bob.manager.id(), Access::read())])
            .await
            .unwrap();

        for (message, _) in output.messages {
            bob.persist_operation(&message).await.unwrap();
            bob.manager.process(&message).await.unwrap();
        }

        let space = tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("pending space resolved")
            .unwrap()
            .unwrap();
        assert_eq!(space.id(), space_id);
    }

    #[tokio::test]
    async fn joined_requires_more_than_pull_access() {
        let alice = TestPeer::new(0).await;
        let bob = TestPeer::new(1).await;

        alice
            .manager
            .register_member(&bob.manager.me().await.unwrap())
            .await
            .unwrap();

        // Alice creates the space with Bob having pull-access.
        let space_id = SpaceId::digest(b"my space");
        let (space, output) = alice
            .manager
            .create_space(space_id, &[(bob.manager.id(), Access::pull())])
            .await
            .unwrap();

        for (message, _) in output.messages {
            bob.persist_operation(&message).await.unwrap();
            bob.manager.process(&message).await.unwrap();
        }

        // Bob knows about the space but hasn't joined it yet.
        let handle = tokio::spawn(bob.manager.space(space_id).joined());
        assert!(bob.manager.has_space(space_id).await.unwrap());
        assert!(!bob.manager.has_joined_space(space_id).await.unwrap());

        // Alice promotes Bob to read access, now they joined.
        let output = space
            .promote(bob.manager.id(), Access::read())
            .await
            .unwrap();

        for (message, _) in output.messages {
            bob.persist_operation(&message).await.unwrap();
            bob.manager.process(&message).await.unwrap();
        }

        let space = tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("pending space resolved")
            .unwrap()
            .unwrap();
        assert_eq!(space.id(), space_id);
        assert!(bob.manager.has_joined_space(space_id).await.unwrap());
    }
}
