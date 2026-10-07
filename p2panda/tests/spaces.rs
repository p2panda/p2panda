// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda::{NetworkId, Node, Topic};
use serde::{Deserialize, Serialize};

struct Swarm {
    network_id: NetworkId,
}

impl Swarm {
    pub fn new() -> Self {
        Self {
            network_id: Topic::random().into(),
        }
    }

    pub async fn spawn_node(&self) -> Node {
        p2panda::builder()
            .network_id(self.network_id)
            .spawn()
            .await
            .unwrap()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SecretData {
    title: String,
    content: String,
}

mod spaces_api {
    use std::collections::HashSet;

    use p2panda::Topic;
    use p2panda::streams::{GroupAction, StreamEvent, StreamFrom};
    use p2panda_auth::{Access, AccessLevel};
    use p2panda_core::test_utils::setup_logging;
    use p2panda_spaces::{MemberId, SpaceEvent};
    use tokio_stream::StreamExt;

    use super::{SecretData, Swarm};

    #[tokio::test]
    async fn space_with_device_group_member() -> Result<(), Box<dyn std::error::Error>> {
        setup_logging();

        let swarm = Swarm::new();

        let panda = swarm.spawn_node().await;

        // Spaces behave like topic-streams, just that they're encrypted towards members.
        let topic = Topic::random();

        // Create a space with only us inside.
        let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await?;

        // Panda receives a space created event for their own action.
        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space {
                inner: SpaceEvent::Created { .. },
                ..
            } = event
            {
                break;
            };
        }

        // We can manage (nested) groups (useful for multi-device, etc.)
        let penguin_laptop = swarm.spawn_node().await;
        let penguin_mobile = swarm.spawn_node().await;

        // Penguin subscribes to the space in order to publish some key bundles.
        let (penguin_laptop_space, mut penguin_laptop_rx) =
            penguin_laptop.space::<SecretData>(topic).await?;
        let (penguin_mobile_space, mut penguin_mobile_rx) =
            penguin_mobile.space::<SecretData>(topic).await?;

        // Panda receives both penguins key bundles.
        let mut expected = HashSet::from([penguin_laptop.id(), penguin_mobile.id()]);
        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Member(verifying_key) = event {
                expected.remove(&verifying_key);
                if expected.is_empty() {
                    break;
                }
            };
        }

        // Penguin creates a device group (on their laptop).
        let (penguin_group, mut penguin_group_rx) = penguin_laptop
            .create_group(&[
                (penguin_laptop.id(), AccessLevel::Write),
                (penguin_mobile.id(), AccessLevel::Read),
            ])
            .await?;

        // Penguin themselves receives the CREATE group event on the group stream.
        while let Some(event) = penguin_group_rx.next().await {
            if let StreamEvent::Group {
                group_id,
                action: GroupAction::Created { .. },
                ..
            } = event
            {
                if group_id == penguin_group.id() {
                    break;
                }
            };
        }

        // Panda wants to add Penguin to the space via their device group. First they need to
        // receive the group via a side-channel. This can be achieved by subscribing directly to
        // the group.
        let (_penguin_group_on_panda, mut penguin_group_on_panda_rx) =
            panda.group(penguin_group.id()).await.unwrap();

        // Panda receives the CREATE group event.
        while let Some(event) = penguin_group_on_panda_rx.next().await {
            if let StreamEvent::Group {
                group_id,
                action: GroupAction::Created { .. },
                ..
            } = event
            {
                if group_id == penguin_group.id() {
                    break;
                }
            };
        }

        // Panda now has penguins device group and can add them to the space.
        panda_space
            .add(penguin_group.id(), AccessLevel::Read)
            .await?;

        // Everyone receives the ADD space event.
        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space {
                members,
                inner: SpaceEvent::Added { .. },
                ..
            } = event
            {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(panda.id(), AccessLevel::Manage)));
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Read)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }

        while let Some(event) = penguin_laptop_rx.next().await {
            if let StreamEvent::Space {
                members,
                inner: SpaceEvent::Added { .. },
                ..
            } = event
            {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(panda.id(), AccessLevel::Manage)));
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Read)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }

        // Penguin mobile never subscribed to the device group, but they still receive this event
        // which required the group to be present. This is due to the fact that once groups become
        // a part of a space, they are always replicated over the space topic as well.
        while let Some(event) = penguin_mobile_rx.next().await {
            if let StreamEvent::Space {
                members,
                inner: SpaceEvent::Added { .. },
                ..
            } = event
            {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(panda.id(), AccessLevel::Manage)));
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Read)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }

        // All nodes arrive at the same state for the space.
        let members = panda_space.members().await?;
        assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Read)));
        assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
        assert!(members.contains(&(panda.id(), AccessLevel::Manage)));

        let members = penguin_laptop_space.members().await?;
        assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Read)));
        assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
        assert!(members.contains(&(panda.id(), AccessLevel::Manage)));

        let members = penguin_mobile_space.members().await?;
        assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Read)));
        assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
        assert!(members.contains(&(panda.id(), AccessLevel::Manage)));

        // Every message published into a space can be decrypted by it's members.
        let message = SecretData {
            title: "My favorite things".to_string(),
            content: "Hello, everyone!".to_string(),
        };
        let ready = panda_space.publish(message.clone()).await?;
        ready.await?;

        // Panda receives the message they sent.
        loop {
            let Some(event) = panda_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Processed { operation, .. } = event else {
                continue;
            };
            assert_eq!(&message, operation.message());
            break;
        }

        // penguin laptop receives the message.
        loop {
            let Some(event) = penguin_laptop_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Processed { operation, .. } = event else {
                continue;
            };
            assert_eq!(&message, operation.message());
            break;
        }

        // penguin mobile receives the message.
        loop {
            let Some(event) = penguin_mobile_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Processed { operation, .. } = event else {
                continue;
            };
            assert_eq!(&message, operation.message());
            break;
        }

        // Panda promotes penguin to have "write" access.
        assert!(
            panda_space
                .promote(penguin_group.id(), AccessLevel::Write)
                .await
                .is_ok()
        );

        assert!(
            panda_space
                .actors()
                .await?
                .contains(&(penguin_group.id(), AccessLevel::Write))
        );

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space {
                members,
                inner: SpaceEvent::Promoted { .. },
                ..
            } = event
            {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Write)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }

        while let Some(event) = penguin_laptop_rx.next().await {
            if let StreamEvent::Space {
                members,
                inner: SpaceEvent::Promoted { .. },
                ..
            } = event
            {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Write)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }

        // Panda demotes penguin to have "read" access.
        assert!(
            panda_space
                .demote(penguin_group.id(), AccessLevel::Read)
                .await
                .is_ok()
        );

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Read)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }

        assert!(
            panda_space
                .actors()
                .await?
                .contains(&(penguin_group.id(), AccessLevel::Read))
        );

        // Penguin laptop also receives the promote and demote.
        while let Some(event) = penguin_laptop_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Read)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }

        Ok(())
    }

    #[tokio::test]
    async fn spaces_sync() -> Result<(), Box<dyn std::error::Error>> {
        setup_logging();

        let swarm = Swarm::new();
        let topic = Topic::random();

        let panda = swarm.spawn_node().await;
        let penguin = swarm.spawn_node().await;

        // Penguin subscribes to the space (and publishes a key bundle).
        let (_penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await?;

        // Panda creates and subscribes to a space.
        let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await?;

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space {
                inner: SpaceEvent::Created { .. },
                ..
            } = event
            {
                break;
            };
        }

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Member(..) = event {
                break;
            };
        }

        // Panda adds penguin as a member of the space.
        //
        // They can do this because they received their key bundle by now.
        panda_space.add(penguin.id(), AccessLevel::Read).await?;

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space { members, inner, .. } = event {
                if let SpaceEvent::Added { .. } = inner {
                    assert_eq!(members.len(), 2);
                    assert!(members.contains(&(penguin.id(), AccessLevel::Read)));
                    break;
                }
            };
        }

        while let Some(event) = penguin_rx.next().await {
            if let StreamEvent::Space { members, inner, .. } = event {
                if let SpaceEvent::Added { .. } = inner {
                    assert_eq!(members.len(), 2);
                    assert!(members.contains(&(penguin.id(), AccessLevel::Read)));
                    break;
                }
            };
        }

        // Panda publishes a message to all members.
        let message = SecretData {
            title: "My favorite things".to_string(),
            content: "Hello, everyone!".to_string(),
        };

        let ready = panda_space.publish(message.clone()).await?;
        assert!(ready.await.is_ok());

        // Panda receives the message they sent.
        loop {
            let Some(event) = panda_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Processed { operation, .. } = event else {
                continue;
            };
            assert_eq!(&message, operation.message());
            break;
        }

        // penguin also receives the message.
        loop {
            let Some(event) = penguin_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Processed { operation, .. } = event else {
                continue;
            };
            assert_eq!(&message, operation.message());
            break;
        }

        Ok(())
    }

    #[tokio::test]
    async fn replay_causally_ordered() {
        setup_logging();

        let swarm = Swarm::new();

        let panda = swarm.spawn_node().await;
        let penguin = swarm.spawn_node().await;

        {
            let penguin_member_info = penguin.me().await.unwrap();
            panda
                .spaces_manager()
                .register_member(&penguin_member_info.into())
                .await
                .unwrap();
        }

        // Creating a space effectively creates two operations:
        //
        // 1. Group CREATE message
        // 2. Space CREATE membership ("key-agreement") message (depends on 1.)
        //
        // Causal order: The second operation points at the first to declare it as a dependency.
        let topic = Topic::random();
        let out = panda
            .spaces_manager()
            .create_space(
                topic,
                &[
                    (panda.id(), Access::write()),
                    (penguin.id(), Access::write()),
                ],
            )
            .await
            .unwrap();

        let mut operations = out
            .messages
            .into_iter()
            .map(|(msg, _)| msg.into_operation());
        let op_1 = operations.next().unwrap();
        let op_2 = operations.next().unwrap();

        // Penguin processes the operations in the "wrong" order (op2 first, then op1).
        let (penguin_space, mut penguin_rx) = penguin.space::<String>(topic).await.unwrap();
        let processed = penguin_space
            .inner_tx()
            .import(futures_util::stream::iter([op_2, op_1]))
            .await
            .unwrap();
        processed.await.unwrap();

        // We expect Penguin to be fine with processing out-of-order operations and normally joining
        // the space.
        while let Some(stream_event) = penguin_rx.next().await {
            if let StreamEvent::Space {
                inner: SpaceEvent::Created { space_id, .. },
                ..
            } = stream_event
            {
                if space_id == topic.into() {
                    break;
                }
            }
        }

        let members: Vec<MemberId> = penguin_space
            .members()
            .await
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert!(members.contains(&panda.id()));
        assert!(members.contains(&penguin.id()));
        assert_eq!(members.len(), 2);

        // Penguin encrypts an application message with the space.
        //
        // Causal order: The resulting operation op3 should point at op2 as a dependency.
        let processed = penguin_space.publish("Chaos!".to_string()).await.unwrap();
        processed.await.unwrap();

        // Processing and receiving the event on application layer will automatically ack it.
        while let Some(stream_event) = penguin_rx.next().await {
            if let StreamEvent::Processed { operation, .. } = stream_event {
                if operation.message() == &"Chaos!".to_string() {
                    break;
                }
            }
        }

        // Create the space stream again and re-play from start.
        drop(penguin_space);
        drop(penguin_rx);

        let (_penguin_space, mut penguin_rx) = penguin
            .space_from::<String>(topic, StreamFrom::Start)
            .await
            .unwrap();

        // We expect the re-played events to arrive in the same, causal order again.
        while let Some(stream_event) = penguin_rx.next().await {
            if let StreamEvent::Space {
                inner: SpaceEvent::Created { space_id, .. },
                ..
            } = stream_event
            {
                if space_id == topic.into() {
                    break;
                }
            }
        }

        while let Some(stream_event) = penguin_rx.next().await {
            if let StreamEvent::Processed { operation, .. } = stream_event {
                if operation.message() == &"Chaos!".to_string() {
                    break;
                }
            }
        }
    }
}

mod spaces_repair_task {
    use std::collections::HashSet;

    use p2panda::streams::{GroupAction, StreamEvent};
    use p2panda::{SpaceEvent, Topic};
    use p2panda_auth::AccessLevel;
    use p2panda_core::test_utils::setup_logging;
    use tokio_stream::StreamExt;

    use super::{SecretData, Swarm};

    #[tokio::test]
    async fn repair_space_sync() {
        // This test demonstrates that the repair task will successfully incorporate concurrently
        // published changes to a member group into a space when they are eventually received.
        setup_logging();

        let swarm = Swarm::new();
        let topic = Topic::random();

        let panda = swarm.spawn_node().await;
        let penguin = swarm.spawn_node().await;
        let penguin_mobile = swarm.spawn_node().await;

        // Panda creates a space.
        let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await.unwrap();

        // Penguin and Penguin (mobile) subscribe to the space and send their key-bundles.
        let (penguin_space, _penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();
        let (penguin_mobile_space, _penguin_mobile_rx) =
            penguin_mobile.space::<SecretData>(topic).await.unwrap();

        // Panda receives both penguins key bundles.
        let mut expected = HashSet::from([penguin.id(), penguin_mobile.id()]);
        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Member(verifying_key) = event {
                expected.remove(&verifying_key);
                if expected.is_empty() {
                    break;
                }
            };
        }

        // Penguin and Penguin (laptop) now unsubscribe from the space for the rest of the test.
        penguin_space.close().await.unwrap();
        penguin_mobile_space.close().await.unwrap();

        // Penguin creates a group but does not subscribing to the space yet.
        let (penguin_group, _) = penguin
            .create_group(&[(penguin.id(), AccessLevel::Manage)])
            .await
            .unwrap();

        // Panda wants to add Penguin to the space via their device group. First they need to
        // receive the group via a side-channel. This can be achieved by subscribing directly to
        // the group.
        let (penguin_group_on_panda, mut penguin_group_on_panda_rx) =
            panda.group(penguin_group.id()).await.unwrap();

        // Panda receives the CREATE group event.
        while let Some(event) = penguin_group_on_panda_rx.next().await {
            if let StreamEvent::Group {
                group_id,
                action: GroupAction::Created { .. },
                ..
            } = event
            {
                if group_id == penguin_group.id() {
                    break;
                }
            };
        }

        // Panda unsubscribes from the group, they won't receive any further group operations.
        penguin_group_on_panda.close().await.unwrap();

        // Panda adds penguin group to the space.
        panda_space
            .add(penguin_group.id(), AccessLevel::Read)
            .await
            .unwrap();

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space { members, inner, .. } = event {
                if let SpaceEvent::Added { .. } = inner {
                    assert_eq!(members.len(), 2);
                    assert!(members.contains(&(penguin.id(), AccessLevel::Read)));
                    break;
                }
            };
        }

        // Penguin now adds a new device to their group.
        //
        // This is happening concurrently to the group being added to the space, therefore panda
        // never incorporated the membership change it reflects.
        penguin_group
            .add(penguin_mobile.id(), AccessLevel::Read)
            .await
            .unwrap();

        // Panda subscribes to the group again and will receive the "ADD" penguin mobile message.
        let (_penguin_group_on_panda, mut penguin_group_on_panda_rx) =
            panda.group(penguin_group.id()).await.unwrap();

        while let Some(event) = penguin_group_on_panda_rx.next().await {
            if let StreamEvent::Group {
                group_id,
                action: GroupAction::Added { .. },
                ..
            } = event
            {
                if group_id == penguin_group.id() {
                    break;
                }
            };
        }

        // The repair task should be triggered and the ADD message incorporated into the space.
        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }
    }

    #[tokio::test]
    async fn repair_space_live() {
        // This test demonstrates that the repair task will successfully incorporate concurrently
        // published changes to a member group into a space when they are eventually received.
        setup_logging();

        let swarm = Swarm::new();
        let topic = Topic::random();

        let panda = swarm.spawn_node().await;
        let penguin = swarm.spawn_node().await;
        let penguin_mobile = swarm.spawn_node().await;

        // Panda creates a space.
        let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await.unwrap();

        // Penguin and Penguin (mobile) subscribe to the space and send their key-bundles.
        let (penguin_space, _penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();
        let (penguin_mobile_space, _penguin_mobile_rx) =
            penguin_mobile.space::<SecretData>(topic).await.unwrap();

        // Panda receives both penguins key bundles.
        let mut expected = HashSet::from([penguin.id(), penguin_mobile.id()]);
        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Member(verifying_key) = event {
                expected.remove(&verifying_key);
                if expected.is_empty() {
                    break;
                }
            };
        }

        // Penguin and Penguin (laptop) now unsubscribe from the space for the rest of the test.
        penguin_space.close().await.unwrap();
        penguin_mobile_space.close().await.unwrap();

        // Penguin creates a group but does not subscribing to the space yet.
        let (penguin_group, _) = penguin
            .create_group(&[(penguin.id(), AccessLevel::Manage)])
            .await
            .unwrap();

        // Panda wants to add Penguin to the space via their device group. First they need to
        // receive the group via a side-channel. This can be achieved by subscribing directly to
        // the group.
        let (_penguin_group_on_panda, mut penguin_group_on_panda_rx) =
            panda.group(penguin_group.id()).await.unwrap();

        // Panda receives the CREATE group event.
        while let Some(event) = penguin_group_on_panda_rx.next().await {
            if let StreamEvent::Group {
                group_id,
                action: GroupAction::Created { .. },
                ..
            } = event
            {
                if group_id == penguin_group.id() {
                    break;
                }
            };
        }

        // Panda adds penguin group to the space.
        panda_space
            .add(penguin_group.id(), AccessLevel::Read)
            .await
            .unwrap();

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space { members, inner, .. } = event {
                if let SpaceEvent::Added { .. } = inner {
                    assert_eq!(members.len(), 2);
                    assert!(members.contains(&(penguin.id(), AccessLevel::Read)));
                    break;
                }
            };
        }

        // Penguin now adds a new device to their group.
        //
        // As penguin is not actually subscribed to the space (they are still unaware they are
        // members) they will not incorporate this change themselves. Panda is still subscribed to
        // the group so they should receive it in live-mode and automatically incorporate it via
        // the repair task being triggered.
        penguin_group
            .add(penguin_mobile.id(), AccessLevel::Read)
            .await
            .unwrap();

        while let Some(event) = penguin_group_on_panda_rx.next().await {
            if let StreamEvent::Group {
                group_id,
                action: GroupAction::Added { .. },
                ..
            } = event
            {
                if group_id == penguin_group.id() {
                    break;
                }
            };
        }

        // The repair task should be triggered and the ADD message incorporated into the space.
        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }
    }
}

mod spaces_api_validation {
    use std::assert_matches;

    use p2panda::spaces::{AddSpaceMemberError, PublishSpaceError, RemoveSpaceMemberError};
    use p2panda::streams::StreamEvent;
    use p2panda::{SigningKey, Topic};
    use p2panda_auth::AccessLevel;
    use p2panda_auth::validation::{AddMemberError, RemoveMemberError, WriteError};
    use p2panda_core::test_utils::setup_logging;
    use p2panda_spaces::SpaceEvent;
    use tokio_stream::StreamExt;

    use crate::Swarm;

    #[tokio::test]
    async fn api_validation() {
        setup_logging();

        let swarm = Swarm::new();
        let topic = Topic::random();

        let panda = swarm.spawn_node().await;

        let (panda_space, mut panda_rx) = panda.create_space::<String>(topic).await.unwrap();

        // Panda can't re-add themselves.
        let result = panda_space.add(panda.id(), AccessLevel::Write).await;
        assert_matches!(
            result.err().unwrap(),
            AddSpaceMemberError::Validation {
                err: AddMemberError::AlreadyAdded,
                ..
            }
        );

        // Panda can't remove a non-member.
        let result = panda_space
            .remove(SigningKey::generate().verifying_key())
            .await;
        assert_matches!(
            result.err().unwrap(),
            RemoveSpaceMemberError::Validation {
                err: RemoveMemberError::NonMember,
                ..
            }
        );

        // Tiger subscribes to the space.
        let tiger = swarm.spawn_node().await;
        let (tiger_space, mut tiger_rx) = tiger.space::<String>(topic).await.unwrap();

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Member(verifying_key) = event {
                if verifying_key == tiger.id() {
                    break;
                }
            };
        }

        // Panda adds tiger with read-only access.
        panda_space
            .add(tiger.id(), AccessLevel::Read)
            .await
            .unwrap();

        while let Some(event) = tiger_rx.next().await {
            if let StreamEvent::Space {
                inner: SpaceEvent::Added { .. },
                ..
            } = event
            {
                break;
            };
        }

        // Tiger can't publish into the space.
        let result = tiger_space.publish("I'm a bit naughty.".to_string()).await;
        assert_matches!(
            result.err().unwrap(),
            PublishSpaceError::Validation {
                err: WriteError::InsufficientAccess,
                ..
            }
        );

        // Panda removes themselves.
        panda_space.remove(panda.id()).await.unwrap();

        let result = panda_space
            .publish("I'm a bit naughty too.".to_string())
            .await;
        assert_matches!(
            result.err().unwrap(),
            PublishSpaceError::Validation {
                err: WriteError::UnrecognisedActor,
                ..
            }
        );
    }
}

mod system_events {
    use p2panda::Topic;
    use p2panda::spaces::InnerGroupEvent;
    use p2panda::streams::SystemEvent;
    use p2panda_auth::AccessLevel;
    use p2panda_core::test_utils::setup_logging;
    use tokio_stream::StreamExt;

    use super::{SecretData, Swarm};

    #[tokio::test]
    async fn group_system_events_via_spaces() {
        setup_logging();

        let swarm = Swarm::new();

        let panda = swarm.spawn_node().await;
        let penguin = swarm.spawn_node().await;

        let mut panda_system_rx = panda.event_stream().await.unwrap();
        let mut penguin_system_rx = penguin.event_stream().await.unwrap();

        let topic = Topic::random();
        let (space, _) = panda.create_space::<SecretData>(topic).await.unwrap();
        let (_space_on_penguin, _) = penguin.space::<SecretData>(topic).await.unwrap();

        // Panda creates a device group.
        let (device_group, _) = panda
            .create_group(&[(panda.id(), AccessLevel::Manage)])
            .await
            .unwrap();

        // And adds it to the space.
        space
            .add(device_group.id(), AccessLevel::Write)
            .await
            .unwrap();

        // Panda receives the device group event on their system stream.
        //
        // Whichever stream a group operation arrives and is processed on the resulting change is
        // emitted as an event on the system event stream. This "enriched" event is only emitted
        // the first time it is processed.
        loop {
            if let Some(SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            }) = panda_system_rx.next().await
            {
                if group_id == device_group.id() {
                    break;
                }
            };
        }
        // Penguin receives the device group event on their system stream.
        loop {
            if let Some(SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            }) = penguin_system_rx.next().await
            {
                if group_id == device_group.id() {
                    break;
                }
            };
        }
    }
}

mod filtered_messages {
    use std::time::Duration;

    use p2panda::Topic;
    use p2panda::streams::StreamEvent;
    use p2panda_auth::AccessLevel;
    use p2panda_core::test_utils::setup_logging;
    use tokio_stream::StreamExt;

    use super::{SecretData, Swarm};

    #[tokio::test]
    async fn concurrently_removed_members_filtered() {
        setup_logging();

        let swarm = Swarm::new();
        let topic = Topic::random();

        let panda = swarm.spawn_node().await;
        let penguin = swarm.spawn_node().await;

        // Panda creates a space.
        let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await.unwrap();

        // Penguin subscribes to the space.
        let (penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Member(member) = event {
                if member == penguin.id() {
                    break;
                }
            };
        }

        // Panda adds Penguin as a member of the space.
        panda_space
            .add(penguin.id(), AccessLevel::Write)
            .await
            .unwrap();

        while let Some(event) = penguin_rx.next().await {
            let StreamEvent::Space { members, .. } = event else {
                continue;
            };

            if members.iter().any(|(member, _)| *member == penguin.id()) {
                break;
            }
        }

        // Penguin publishes a message to all members.
        let message = SecretData {
            title: "My favorite things".to_string(),
            content: "Hello, everyone!".to_string(),
        };

        let ready = penguin_space.publish(message.clone()).await.unwrap();
        assert!(ready.await.is_ok());

        // Panda receives the message from Penguin.
        loop {
            let Some(event) = panda_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Processed { operation, .. } = event else {
                continue;
            };
            assert_eq!(&message, operation.message());
            break;
        }

        let penguin_id = penguin.id();

        // Penguin unsubscribes from the space.
        penguin_space.close().await.unwrap();

        // Panda removes Penguin.
        panda_space
            .remove(penguin_id)
            .await
            .expect("panda removes penguin");

        // Panda unsubscribes.
        panda_space.close().await.unwrap();

        // Penguin subscribes to the space again and immediately publishes a new message, before
        // they received panda's remove message.
        let (penguin_space, _penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();

        let message = SecretData {
            title: "Hurtful words".to_string(),
            content: "Panda can't jump very high".to_string(),
        };

        let ready = penguin_space
            .publish(message.clone())
            .await
            .expect("can publish message to group");
        assert!(ready.await.is_ok());

        // Panda subscribes again.
        let (_panda_space, mut panda_rx) = panda.space::<SecretData>(topic).await.unwrap();

        // And manually adds penguin to the allow-list as removed members are automatically blocked.
        panda.sync_block_list().allow_topic(penguin_id, topic).await;

        // Panda will be sent the second message from penguin, however it will not be forwarded to
        // the app layer as they know penguin has been removed (concurrent to the application
        // message being published).
        let mut penguin_message_filtered = true;
        let sleep = tokio::time::sleep(Duration::from_secs(3));
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                event = panda_rx.next() => {
                    match event {
                        Some(StreamEvent::Processed { .. }) => penguin_message_filtered = false,
                        None => panic!("unexpected stream closure"),
                        _ => (),
                    }
                }
                _ = &mut sleep => {
                    break;
                }
            }
        }

        assert!(penguin_message_filtered);
    }

    #[tokio::test]
    async fn causally_later_removed_members_not_filtered() {
        setup_logging();

        let swarm = Swarm::new();
        let topic = Topic::random();

        let panda = swarm.spawn_node().await;
        let penguin = swarm.spawn_node().await;
        let tiger = swarm.spawn_node().await;

        // Panda creates a space.
        let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await.unwrap();

        // Penguin subscribes to the space.
        let (penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Member(member) = event {
                if member == penguin.id() {
                    break;
                }
            };
        }

        // Panda adds Penguin as a member of the space.
        panda_space
            .add(penguin.id(), AccessLevel::Write)
            .await
            .unwrap();

        while let Some(event) = penguin_rx.next().await {
            let StreamEvent::Space { members, .. } = event else {
                continue;
            };

            if members.iter().any(|(member, _)| *member == penguin.id()) {
                break;
            }
        }

        // Penguin publishes a message to all members.
        let message = SecretData {
            title: "My favorite things".to_string(),
            content: "Hello, everyone!".to_string(),
        };

        let ready = penguin_space.publish(message.clone()).await.unwrap();
        assert!(ready.await.is_ok());

        // Panda receives the message from Penguin.
        loop {
            let Some(event) = panda_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Processed { operation, .. } = event else {
                continue;
            };
            assert_eq!(&message, operation.message());
            break;
        }

        // Panda removes Penguin.
        panda_space.remove(penguin.id()).await.unwrap();

        // Tiger subscribes to the space.
        let (_tiger_space, mut tiger_rx) = tiger.space::<SecretData>(topic).await.unwrap();

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Member(member) = event {
                if member == tiger.id() {
                    break;
                }
            };
        }

        // Panda adds Tiger as a member of the space.
        panda_space
            .add(tiger.id(), AccessLevel::Read)
            .await
            .unwrap();

        // Tiger receives the message from Penguin even though they have since been removed.
        loop {
            let Some(event) = tiger_rx.next().await else {
                panic!("unexpected stream closure");
            };

            let StreamEvent::Processed { operation, .. } = event else {
                continue;
            };

            assert_eq!(&message, operation.message());
            break;
        }
    }
}

mod members {
    use p2panda::Topic;
    use p2panda::streams::StreamEvent;
    use p2panda_auth::AccessLevel;
    use p2panda_core::test_utils::setup_logging;
    use tokio_stream::StreamExt;

    use super::Swarm;

    // 1. Node A creates space S with {A, B, C, D} inside
    // 2. Node B removes C from S
    //
    // Node B needs to inform A & D about the new secret after removing C and needs a key bundle of
    // D to do that. If all member logs are correctly associated, B should have received it
    // indirectly via A.
    #[tokio::test]
    async fn indirect_members_log_sync() {
        setup_logging();

        let swarm = Swarm::new();
        let topic = Topic::random();

        let node_a = swarm.spawn_node().await;
        let node_b = swarm.spawn_node().await;
        let node_c = swarm.spawn_node().await;
        let node_d = swarm.spawn_node().await;

        let (node_a_space, mut node_a_rx) = node_a.create_space::<String>(topic).await.unwrap();

        // Nodes C and D come online to sync their member logs with A. Node A will from now on
        // "carry" their logs. We will shut down C and D directly afterwards to make sure this is
        // ensured and A stays the only source of C or D's key bundle in the network.
        let (node_c_space, node_c_rx) = node_c.space::<String>(topic).await.unwrap();
        let (node_d_space, node_d_rx) = node_d.space::<String>(topic).await.unwrap();

        let mut required_key_bundles = vec![node_c.id(), node_d.id()];

        while let Some(event) = node_a_rx.next().await {
            let StreamEvent::Member(member_id) = event else {
                continue;
            };

            required_key_bundles.retain(|id| &member_id != id);

            if required_key_bundles.is_empty() {
                break;
            }
        }

        drop(node_c_space);
        drop(node_c_rx);

        drop(node_d_space);
        drop(node_d_rx);

        // Node A brings everyone into the space. The space has the members {A, B, C, D} now.
        node_a
            .register_member(node_b.me().await.unwrap())
            .await
            .unwrap();

        node_a_space
            .add(node_b.id(), AccessLevel::Manage)
            .await
            .unwrap();

        node_a_space
            .add(node_c.id(), AccessLevel::Write)
            .await
            .unwrap();

        node_a_space
            .add(node_d.id(), AccessLevel::Write)
            .await
            .unwrap();

        // Wait until B finished processing being added to the space and received all required
        // key bundles AND got informed about all other members (C and D) being added.
        let (node_b_space, mut node_b_rx) = node_b.space::<String>(topic).await.unwrap();

        let mut required_key_bundles = vec![node_a.id(), node_d.id()];
        let mut required_adds = vec![node_b.id(), node_c.id(), node_d.id()];

        while let Some(event) = node_b_rx.next().await {
            match event {
                StreamEvent::Space { members, .. } => {
                    for (member_id, _) in members {
                        required_adds.retain(|id| &member_id != id);
                    }
                }
                StreamEvent::Member(member_id) => {
                    required_key_bundles.retain(|id| &member_id != id);
                }
                _ => continue,
            }

            if required_key_bundles.is_empty() && required_adds.is_empty() {
                break;
            }
        }

        // B wants to remove C and needs the key bundles of D to do this since they've never sent a
        // direct message to D. Note that B doesn't need a key bundle for A because A already
        // initiated a 2SM session with B when they've been added to the space.
        node_b_space.remove(node_c.id()).await.unwrap();
    }
}

mod sync_authorisation {
    use p2panda::Topic;
    use p2panda::streams::{StreamEvent, SystemEvent};
    use p2panda_auth::AccessLevel;
    use p2panda_core::test_utils::setup_logging;
    use p2panda_net::sync::authoriser::SyncBlockListEvent;
    use tokio_stream::StreamExt;

    use super::{SecretData, Swarm};

    #[tokio::test]
    async fn member_allow_and_block() {
        setup_logging();

        let swarm = Swarm::new();
        let topic = Topic::random();

        let panda = swarm.spawn_node().await;
        let mut panda_system_rx = panda.event_stream().await.unwrap();
        let penguin = swarm.spawn_node().await;

        // Panda creates a space.
        let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await.unwrap();

        // Penguin subscribes to the space.
        let (penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Member(member) = event {
                if member == penguin.id() {
                    break;
                }
            };
        }

        // Panda adds Penguin as a member of the space.
        let penguin_id = penguin.id();
        panda_space
            .add(penguin_id, AccessLevel::Write)
            .await
            .unwrap();

        while let Some(event) = penguin_rx.next().await {
            let StreamEvent::Space { members, .. } = event else {
                continue;
            };

            if members.iter().any(|(member, _)| *member == penguin.id()) {
                break;
            }
        }

        // Penguin unsubscribes from the space.
        penguin_space.close().await.unwrap();

        // Panda removes Penguin.
        panda_space
            .remove(penguin_id)
            .await
            .expect("panda removes penguin");

        let (penguin_space, _penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();

        while let Some(event) = panda_system_rx.next().await {
            let SystemEvent::SyncAuthoriser(SyncBlockListEvent::Blocked {
                topic: topic_inner,
                remote_node_id,
            }) = event
            else {
                continue;
            };

            assert_eq!(remote_node_id, penguin_id);
            assert_eq!(topic_inner, topic);
            break;
        }

        penguin_space.close().await.unwrap();

        // Panda adds Penguin again.
        panda_space
            .add(penguin_id, AccessLevel::Read)
            .await
            .expect("panda removes penguin");

        let (_penguin_space, _penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();

        while let Some(event) = panda_system_rx.next().await {
            let SystemEvent::SyncAuthoriser(SyncBlockListEvent::Allowed {
                topic: topic_inner,
                remote_node_id,
            }) = event
            else {
                continue;
            };

            assert_eq!(remote_node_id, penguin_id);
            assert_eq!(topic_inner, topic);
            break;
        }
    }
}

mod spaces_groups_membership {
    use p2panda::operation::Extensions;
    use p2panda::spaces::{Group, InnerGroupEvent};
    use p2panda::streams::{StreamEvent, SystemEvent};
    use p2panda::{Node, Topic};
    use p2panda_auth::AccessLevel;
    use p2panda_core::test_utils::setup_logging;
    use p2panda_spaces::{SpaceEvent, SpacesStoreState};
    use p2panda_store::spaces::{SpacesStore, SqliteSpacesStore};
    use p2panda_store::tx_unwrap;
    use tokio_stream::StreamExt;

    use crate::Swarm;

    use super::SecretData;

    async fn spawn_node_with_device_group(swarm: &Swarm) -> (Node, Group) {
        let node = swarm.spawn_node().await;
        let (device_group, _) = node
            .create_group(&[(node.id(), AccessLevel::Manage)])
            .await
            .unwrap();
        (node, device_group)
    }

    #[tokio::test]
    async fn add_device_groups_to_team_in_space() {
        setup_logging();

        let swarm = Swarm::new();
        let topic = Topic::random();

        // Alice, Bob and Claire each create a device group with only themselves inside.
        let (alice, alice_device) = spawn_node_with_device_group(&swarm).await;
        let (bob, bob_device) = spawn_node_with_device_group(&swarm).await;
        let (claire, claire_device) = spawn_node_with_device_group(&swarm).await;

        let _alice_bob_device = alice.group(bob_device.id()).await.unwrap();

        let mut alice_system_rx = alice.event_stream().await.unwrap();
        let mut bob_system_rx = bob.event_stream().await.unwrap();

        while let Some(event) = alice_system_rx.next().await {
            if let SystemEvent::Groups { group_id, .. } = event {
                if group_id == bob_device.id() {
                    break;
                }
            };
        }

        // Alice creates a space.
        let (alice_space, mut alice_rx) = alice.create_space::<SecretData>(topic).await.unwrap();
        let space_group_id = alice_space.group_id().await.unwrap();

        let store = SqliteSpacesStore::<Extensions>::new(alice.store());
        let y: SpacesStoreState<()> =
            tx_unwrap!(store, { store.get_space_state_tx(&alice_space.id()).await })
                .unwrap()
                .unwrap();
        assert_eq!(y.groups_y.inner.operations.len(), 1);

        while let Some(event) = alice_rx.next().await {
            if let StreamEvent::Space {
                inner: SpaceEvent::Created { .. },
                ..
            } = event
            {
                break;
            };
        }

        // Alice creates a team group with their device group as a member.
        //
        // NOTE: As groups can't be assigned manager access level yet we have to add Alice
        // directly as a member as well.
        let (team, _team_rx) = alice
            .create_group(&[
                (alice_device.id(), AccessLevel::Write),
                (alice.id(), AccessLevel::Manage),
            ])
            .await
            .unwrap();

        // Alice receives the team group.
        while let Some(event) = alice_system_rx.next().await {
            if let SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            } = event
            {
                if group_id == team.id() {
                    break;
                }
            };
        }

        // Alice adds the team group as a member of the space.
        alice_space
            .add(team.id(), AccessLevel::Write)
            .await
            .unwrap();

        while let Some(event) = alice_system_rx.next().await {
            if let SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Added { added, .. },
                ..
            } = event
            {
                if group_id == space_group_id && added.id() == team.id() {
                    break;
                }
            };
        }

        // Bob subscribes to the space.
        let (bob_space, mut bob_rx) = bob.space::<SecretData>(topic).await.unwrap();

        // Alice receives Bob's key bundle.
        while let Some(event) = alice_rx.next().await {
            if let StreamEvent::Member(member) = event {
                if member == bob.id() {
                    break;
                }
            };
        }

        // Alice adds Bob's device group to the team group.
        //
        // NOTE: Integrating this change into the space is handled by the repair task.
        team.add(bob_device.id(), AccessLevel::Write).await.unwrap();

        // Alice and Bob both arrive at the same membership state.
        loop {
            let Some(event) = alice_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Space { members, .. } = event else {
                continue;
            };
            if !members.iter().any(|(member, _)| *member == bob.id()) {
                continue;
            }
            assert_eq!(members.len(), 2);
            assert!(members.contains(&(alice.id(), AccessLevel::Manage)));
            assert!(members.contains(&(bob.id(), AccessLevel::Write)));
            break;
        }

        // Bob receives space group.
        let mut space_group_seen = false;
        let mut alice_device_group_seen = false;
        let mut team_group_seen = false;
        while let Some(event) = bob_system_rx.next().await {
            if let SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            } = event
            {
                if group_id == space_group_id {
                    space_group_seen = true;
                }

                if group_id == alice_device.id() {
                    alice_device_group_seen = true;
                }

                if group_id == team.id() {
                    team_group_seen = true;
                }

                if space_group_seen && alice_device_group_seen && team_group_seen {
                    break;
                }
            };
        }

        loop {
            let Some(event) = bob_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Space { members, .. } = event else {
                continue;
            };
            if !members.iter().any(|(member, _)| *member == bob.id()) {
                continue;
            }
            assert_eq!(members.len(), 2);
            assert!(members.contains(&(alice.id(), AccessLevel::Manage)));
            assert!(members.contains(&(bob.id(), AccessLevel::Write)));
            break;
        }

        bob_space.close().await.unwrap();

        // Claire subscribes to the space.
        let (_claire_space, mut claire_rx) = claire.space::<SecretData>(topic).await.unwrap();

        // Alice subscribes to claire's device group.
        let _alice_claire_device = alice.group(claire_device.id()).await.unwrap();

        // Alice receives Claire's device group.
        while let Some(event) = alice_system_rx.next().await {
            if let SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            } = event
            {
                if group_id == claire_device.id() {
                    break;
                }
            };
        }

        // Alice receives Claire's key bundle.
        while let Some(event) = alice_rx.next().await {
            if let StreamEvent::Member(member) = event {
                if member == claire.id() {
                    break;
                }
            };
        }

        // Alice adds Claire's device group to the team group.
        team.add(claire_device.id(), AccessLevel::Read)
            .await
            .unwrap();

        let (_bob_space, mut bob_rx) = bob.space::<SecretData>(topic).await.unwrap();

        // Alice, Bob and Claire all arrive at the same membership state.
        loop {
            let Some(event) = alice_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Space { members, .. } = event else {
                continue;
            };
            if !members.iter().any(|(member, _)| *member == claire.id()) {
                continue;
            }
            assert_eq!(members.len(), 3);
            assert!(members.contains(&(alice.id(), AccessLevel::Manage)));
            assert!(members.contains(&(bob.id(), AccessLevel::Write)));
            assert!(members.contains(&(claire.id(), AccessLevel::Read)));
            break;
        }

        loop {
            let Some(event) = bob_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Space { members, .. } = event else {
                continue;
            };
            if !members.iter().any(|(member, _)| *member == claire.id()) {
                continue;
            }
            assert_eq!(members.len(), 3);
            assert!(members.contains(&(alice.id(), AccessLevel::Manage)));
            assert!(members.contains(&(bob.id(), AccessLevel::Write)));
            assert!(members.contains(&(claire.id(), AccessLevel::Read)));
            break;
        }

        loop {
            let Some(event) = claire_rx.next().await else {
                panic!("unexpected stream closure");
            };
            let StreamEvent::Space { members, .. } = event else {
                continue;
            };
            if !members.iter().any(|(member, _)| *member == claire.id()) {
                continue;
            }
            assert_eq!(members.len(), 3);
            assert!(members.contains(&(alice.id(), AccessLevel::Manage)));
            assert!(members.contains(&(bob.id(), AccessLevel::Write)));
            assert!(members.contains(&(claire.id(), AccessLevel::Read)));
            break;
        }
    }
}
