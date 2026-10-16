// SPDX-License-Identifier: MIT OR Apache-2.0

//! Handles to await spaces, groups and members which might not be created or joined yet.
use p2panda_auth::traits::Conditions;
use p2panda_encryption::key_bundle::LongTermKeyBundle;
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::PreKeyBundlesStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore};

use crate::event::{GroupEvent, SpaceEvent};
use crate::group::Group;
use crate::manager::{Manager, ManagerError};
use crate::space::Space;
use crate::utils::{is_member, is_secret_member};
use crate::{AuthMessage, Forge, GroupId, MemberId, SpaceId, SpacesStoreState};

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

#[derive(Debug)]
pub struct PendingGroup<S, F, C> {
    id: GroupId,
    manager_ref: Manager<S, F, C>,
}

impl<S, F, C> PendingGroup<S, F, C> {
    pub(crate) fn new(manager_ref: Manager<S, F, C>, id: GroupId) -> Self {
        Self { id, manager_ref }
    }

    pub fn id(&self) -> GroupId {
        self.id
    }
}

impl<S, F, C> PendingGroup<S, F, C>
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
    /// Wait until the group is known to us.
    ///
    /// Resolves as soon as we processed the "create" message of the group, even if we're not a
    /// member of it.
    ///
    /// This never resolves if the group never arrives, consider wrapping it in a timeout.
    pub async fn created(self) -> Result<Group<S, F, C>, ManagerError<F, C>> {
        let id = self.id;

        // Subscribe _before_ checking the store, otherwise we might miss a notification.
        let waiter = self.manager_ref.waiters().groups.wait_for(
            move |event| matches!(event, GroupEvent::Created { group_id, .. } if *group_id == id),
        );

        // Group exists already.
        if !self.manager_ref.has_group(id).await? {
            waiter.await;
        }

        Ok(Group::new(self.manager_ref, id))
    }

    /// Wait until we joined the group with any access level, either directly or through another
    /// group which is a member of it.
    ///
    /// This never resolves if we're never added or been removed, consider wrapping it in a timeout.
    pub async fn joined(self) -> Result<Group<S, F, C>, ManagerError<F, C>> {
        let id = self.id;
        let me = self.manager_ref.id();

        // Subscribe _before_ checking the store, otherwise we might miss a notification.
        let waiter = self.manager_ref.waiters().groups.wait_for(move |event| {
            matches!(event, GroupEvent::Created { .. } | GroupEvent::Added { .. })
                && event
                    .members(id)
                    .is_some_and(|members| is_member(members, me))
        });

        // We've joined already.
        if !self.manager_ref.has_joined_group(id).await? {
            waiter.await;
        }

        Ok(Group::new(self.manager_ref, id))
    }
}

#[derive(Debug)]
pub struct PendingMember<S, F, C> {
    id: MemberId,
    manager_ref: Manager<S, F, C>,
}

impl<S, F, C> PendingMember<S, F, C> {
    pub(crate) fn new(manager_ref: Manager<S, F, C>, id: MemberId) -> Self {
        Self { id, manager_ref }
    }

    pub fn id(&self) -> MemberId {
        self.id
    }
}

impl<S, F, C> PendingMember<S, F, C>
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
    /// Wait until a valid key bundle of the member was registered and return it.
    ///
    /// This never resolves if no key bundle arrives, consider wrapping it in a timeout.
    pub async fn registered(self) -> Result<LongTermKeyBundle, ManagerError<F, C>> {
        let id = self.id;

        loop {
            // Subscribe _before_ checking the store, otherwise we might miss a notification.
            let waiter = self
                .manager_ref
                .waiters()
                .members
                .wait_for(move |member| member.id() == id);

            if let Some(key_bundle) = self.manager_ref.member_key_bundle(id).await? {
                return Ok(key_bundle);
            }

            waiter.await;
        }
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
    async fn space_created_resolves_immediately() {
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
    async fn space_created_resolves_on_message() {
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

    #[tokio::test]
    async fn group_created_resolves_on_message() {
        let alice = TestPeer::new(0).await;
        let bob = TestPeer::new(1).await;

        let (group, output) = alice.manager.create_group(&[]).await.unwrap();

        // Bob waits for a group which doesn't exist yet.
        let handle = tokio::spawn(bob.manager.group(group.id()).created());

        bob.persist_operation(&output.message).await.unwrap();
        bob.manager.process(&output.message).await.unwrap();

        let bob_group = tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("pending group resolved")
            .unwrap()
            .unwrap();
        assert_eq!(bob_group.id(), group.id());

        // Bob knows about the group but is not a member.
        assert!(bob.manager.has_group(group.id()).await.unwrap());
        assert!(!bob.manager.has_joined_group(group.id()).await.unwrap());
    }

    #[tokio::test]
    async fn group_joined_through_nested() {
        let alice = TestPeer::new(0).await;
        let bob = TestPeer::new(1).await;
        let bob_id = bob.manager.id();

        // Alice creates a parent and a child group and adds the child to the parent.
        let alice_manage = [(alice.manager.id(), Access::manage())];
        let (parent, parent_output) = alice.manager.create_group(&alice_manage).await.unwrap();
        let (child, child_output) = alice.manager.create_group(&alice_manage).await.unwrap();
        let add_child_output = parent.add(child.id(), Access::read()).await.unwrap();

        // Bob processes the resulting messages.
        for message in [
            parent_output.message,
            child_output.message,
            add_child_output.message,
        ] {
            bob.persist_operation(&message).await.unwrap();
            bob.manager.process(&message).await.unwrap();
        }

        // Bob waits to join the parent group.
        let joined = bob.manager.group(parent.id()).joined();
        tokio::pin!(joined);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut joined)
                .await
                .is_err()
        );

        // Alice adds Bob with pull access to the child group, which makes Bob a member of the
        // parent group as well.
        let add_bob_output = child.add(bob_id, Access::pull()).await.unwrap();
        bob.persist_operation(&add_bob_output.message)
            .await
            .unwrap();
        bob.manager.process(&add_bob_output.message).await.unwrap();

        let bob_group = tokio::time::timeout(Duration::from_secs(1), joined)
            .await
            .expect("pending group resolved")
            .unwrap();
        assert_eq!(bob_group.id(), parent.id());
        assert!(bob.manager.has_joined_group(parent.id()).await.unwrap());
    }

    #[tokio::test]
    async fn member_resolves_immediately() {
        let alice = TestPeer::new(0).await;
        let bob = TestPeer::new(1).await;

        let bob_member = bob.manager.me().await.unwrap();
        alice.manager.register_member(&bob_member).await.unwrap();

        let key_bundle = alice
            .manager
            .member(bob.manager.id())
            .registered()
            .await
            .unwrap();
        assert_eq!(&key_bundle, bob_member.key_bundle());
    }

    #[tokio::test]
    async fn member_resolves_on_message() {
        let alice = TestPeer::new(0).await;
        let bob = TestPeer::new(1).await;

        // Alice waits for Bob's key bundle.
        let registered = alice.manager.member(bob.manager.id()).registered();

        // Bob publishes their key bundle and Alice processes it.
        let message = bob.manager.key_bundle_message().await.unwrap();
        alice.persist_operation(&message).await.unwrap();
        alice.manager.process(&message).await.unwrap();

        let key_bundle = tokio::time::timeout(Duration::from_secs(1), registered)
            .await
            .expect("pending member resolved")
            .unwrap();
        assert_eq!(&key_bundle, bob.manager.me().await.unwrap().key_bundle());
    }
}
