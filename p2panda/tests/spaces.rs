// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda::{NetworkId, Node};
use serde::{Deserialize, Serialize};

async fn spawn(network_id: NetworkId) -> Node {
    p2panda::builder()
        .network_id(network_id)
        .spawn()
        .await
        .unwrap()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SecretData {
    title: String,
    content: String,
}

mod spaces_api {
    use std::collections::HashSet;

    use p2panda::Topic;
    use p2panda::spaces::InnerGroupEvent;
    use p2panda::streams::{StreamEvent, SystemEvent};
    use p2panda_auth::AccessLevel;
    use p2panda_core::test_utils::setup_logging;
    use p2panda_spaces::SpaceEvent;
    use tokio_stream::StreamExt;

    use super::{SecretData, spawn};

    #[tokio::test]
    async fn create_space_for_multiple_members() -> Result<(), Box<dyn std::error::Error>> {
        setup_logging();

        let network_id = Topic::random().into();

        let panda = spawn(network_id).await;
        let mut panda_system_rx = panda.event_stream().await?;

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
        let penguin_laptop = spawn(network_id).await;
        let penguin_mobile = spawn(network_id).await;
        let mut penguin_mobile_system_rx = penguin_mobile.event_stream().await?;

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
        let penguin = penguin_laptop
            .create_group(&[
                (penguin_laptop.id(), AccessLevel::Write),
                (penguin_mobile.id(), AccessLevel::Read),
            ])
            .await?;

        // Panda receives the group.
        while let Some(event) = panda_system_rx.next().await {
            if let SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            } = event
            {
                if group_id == penguin.id() {
                    break;
                }
            };
        }

        // Penguin mobile receives the group.
        while let Some(event) = penguin_mobile_system_rx.next().await {
            if let SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            } = event
            {
                if group_id == penguin.id() {
                    break;
                }
            };
        }

        panda_space.add(penguin.id(), AccessLevel::Read).await?;

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(panda.id(), AccessLevel::Manage)));
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Read)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }

        while let Some(event) = penguin_laptop_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(panda.id(), AccessLevel::Manage)));
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Read)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }

        while let Some(event) = penguin_mobile_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(panda.id(), AccessLevel::Manage)));
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Read)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                break;
            };
        }

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
                .promote(penguin.id(), AccessLevel::Write)
                .await
                .is_ok()
        );

        assert!(
            panda_space
                .actors()
                .await?
                .contains(&(penguin.id(), AccessLevel::Write))
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
                .demote(penguin.id(), AccessLevel::Read)
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
                .contains(&(penguin.id(), AccessLevel::Read))
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

        let network_id = Topic::random().into();
        let topic = Topic::random();

        let panda = spawn(network_id).await;
        let penguin = spawn(network_id).await;

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
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 2);
                assert!(members.contains(&(penguin.id(), AccessLevel::Read)));
                break;
            };
        }

        while let Some(event) = penguin_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 2);
                assert!(members.contains(&(penguin.id(), AccessLevel::Read)));
                break;
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
}

mod spaces_repair_task {
    use p2panda::Topic;
    use p2panda::spaces::InnerGroupEvent;
    use p2panda::streams::{StreamEvent, SystemEvent};
    use p2panda_auth::AccessLevel;
    use p2panda_core::test_utils::setup_logging;
    use p2panda_spaces::SpaceEvent;
    use tokio_stream::StreamExt;

    use super::{SecretData, spawn};

    #[tokio::test]
    async fn sync_repair_space() {
        setup_logging();

        let network_id = Topic::random().into();
        let topic = Topic::random();

        let panda = spawn(network_id).await;
        let mut panda_system_rx = panda.event_stream().await.unwrap();
        let penguin = spawn(network_id).await;
        let mut penguin_system_rx = penguin.event_stream().await.unwrap();

        // Penguin creates a group before subscribing to the space.
        let penguin_group = penguin
            .create_group(&[(penguin.id(), AccessLevel::Manage)])
            .await
            .unwrap();

        // They then subscribe, as does panda.
        let (_penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();
        let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await.unwrap();

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space {
                inner: SpaceEvent::Created { .. },
                ..
            } = event
            {
                break;
            };
        }

        // Panda receives the group.
        while let Some(event) = panda_system_rx.next().await {
            if let SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            } = event
            {
                if group_id == penguin_group.id() {
                    break;
                }
            };
        }

        // Penguin receives the group.
        while let Some(event) = penguin_system_rx.next().await {
            if let SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            } = event
            {
                if group_id == penguin_group.id() {
                    break;
                }
            };
        }
        // We expect panda to be able to add penguin group as a space member now.
        panda_space
            .add(penguin_group.id(), AccessLevel::Read)
            .await
            .unwrap();

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 2);
                assert!(members.contains(&(penguin.id(), AccessLevel::Read)));
                break;
            };
        }

        while let Some(event) = penguin_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 2);
                assert!(members.contains(&(penguin.id(), AccessLevel::Read)));
                break;
            };
        }
    }

    #[tokio::test]
    async fn live_repair_space() {
        setup_logging();

        let network_id = Topic::random().into();
        let topic = Topic::random();

        let panda = spawn(network_id).await;
        let mut panda_system_rx = panda.event_stream().await.unwrap();
        let penguin = spawn(network_id).await;

        // Penguin subscribes to the space.
        let (_penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();
        let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await.unwrap();

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space {
                inner: SpaceEvent::Created { .. },
                ..
            } = event
            {
                break;
            };
        }

        // And then creates a group.
        let penguin_group = penguin
            .create_group(&[(penguin.id(), AccessLevel::Manage)])
            .await
            .unwrap();

        // Panda receives the group.
        while let Some(event) = panda_system_rx.next().await {
            if let SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            } = event
            {
                if group_id == penguin_group.id() {
                    break;
                }
            };
        }

        // We expect panda to be able to add penguin group to the space.
        panda_space
            .add(penguin_group.id(), AccessLevel::Read)
            .await
            .unwrap();

        while let Some(event) = panda_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 2);
                assert!(members.contains(&(penguin.id(), AccessLevel::Read)));
                break;
            };
        }

        while let Some(event) = penguin_rx.next().await {
            if let StreamEvent::Space { members, .. } = event {
                assert_eq!(members.len(), 2);
                assert!(members.contains(&(penguin.id(), AccessLevel::Read)));
                break;
            };
        }
    }
}

mod spaces_api_validation {
    use std::assert_matches;

    use p2panda::spaces::{
        AddGroupMemberError, AddSpaceMemberError, InnerGroupEvent, PublishSpaceError,
        RemoveGroupMemberError, RemoveSpaceMemberError,
    };
    use p2panda::streams::{StreamEvent, SystemEvent};
    use p2panda::{SigningKey, Topic};
    use p2panda_auth::AccessLevel;
    use p2panda_auth::validation::{AddMemberError, RemoveMemberError, WriteError};
    use p2panda_core::test_utils::setup_logging;
    use p2panda_spaces::SpaceEvent;
    use tokio_stream::StreamExt;

    use super::{SecretData, spawn};

    #[tokio::test]
    async fn api_validation() {
        setup_logging();

        let network_id = Topic::random().into();
        let topic = Topic::random();

        let panda = spawn(network_id).await;

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
        let tiger = spawn(network_id).await;
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

    #[tokio::test]
    async fn groups_api_validation() {
        setup_logging();

        let network_id = Topic::random().into();

        let panda = spawn(network_id).await;
        let lion = spawn(network_id).await;
        let tiger = spawn(network_id).await;

        let topic = Topic::random();

        // Having a space in this test is only required to sync the group operations.
        let (_panda_space, _panda_rx) = panda.create_space::<SecretData>(topic).await.unwrap();
        let (_tiger_space, _tiger_rx) = tiger.space::<SecretData>(topic).await.unwrap();
        let (_lion_space, _lion_rx) = lion.space::<SecretData>(topic).await.unwrap();

        let mut lion_system_rx = lion.event_stream().await.unwrap();
        let mut tiger_system_rx = tiger.event_stream().await.unwrap();

        let panda_group = panda
            .create_group(&[
                (panda.id(), AccessLevel::Manage),
                (lion.id(), AccessLevel::Read),
            ])
            .await
            .unwrap();

        // Panda can't re-add themselves.
        let result = panda_group.add(panda.id(), AccessLevel::Write).await;
        assert_matches!(
            result.err().unwrap(),
            AddGroupMemberError::Validation {
                err: AddMemberError::AlreadyAdded,
                ..
            }
        );

        // Panda can't remove a non-member.
        let result = panda_group
            .remove(SigningKey::generate().verifying_key())
            .await;
        assert_matches!(
            result.err().unwrap(),
            RemoveGroupMemberError::Validation {
                err: RemoveMemberError::NonMember,
                ..
            }
        );

        // Lion receives the group event on their system stream.
        loop {
            if let Some(SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            }) = lion_system_rx.next().await
            {
                if group_id == panda_group.id() {
                    break;
                }
            };
        }

        // Tiger receives the group event on their system stream.
        loop {
            if let Some(SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            }) = tiger_system_rx.next().await
            {
                if group_id == panda_group.id() {
                    break;
                }
            };
        }

        // Tiger isn't a recognized group actor.
        let panda_group_on_tiger = tiger.group(panda_group.id()).await.unwrap().unwrap();
        let result = panda_group_on_tiger
            .add(tiger.id(), AccessLevel::Write)
            .await;
        assert_matches!(
            result.err().unwrap(),
            AddGroupMemberError::Validation {
                err: AddMemberError::UnrecognisedActor,
                ..
            }
        );

        // Lion doesn't have required access level.
        let panda_group_on_lion = lion.group(panda_group.id()).await.unwrap().unwrap();
        let result = panda_group_on_lion.remove(panda.id()).await;
        assert_matches!(
            result.err().unwrap(),
            RemoveGroupMemberError::Validation {
                err: RemoveMemberError::InsufficientAccess,
                ..
            }
        );
    }
}

mod spaces_events {
    use p2panda::Topic;
    use p2panda::spaces::{GroupEvent, InnerGroupEvent};
    use p2panda::streams::SystemEvent;
    use p2panda_auth::AccessLevel;
    use p2panda_core::test_utils::setup_logging;
    use tokio_stream::StreamExt;

    use super::{SecretData, spawn};

    #[tokio::test]
    async fn group_events() {
        setup_logging();

        let network_id = Topic::random().into();

        let panda = spawn(network_id).await;
        let mut panda_system_rx = panda.event_stream().await.unwrap();

        let topic = Topic::random();

        // Create a space with only us inside. Having a space in this test is only required to sync
        // the group operations.
        let (_panda_space, _panda_rx) = panda.create_space::<SecretData>(topic).await.unwrap();

        let penguin_laptop = spawn(network_id).await;
        let penguin_mobile = spawn(network_id).await;
        let mut penguin_laptop_system_rx = penguin_laptop.event_stream().await.unwrap();

        let (_penguin_laptop_space, _penguin_laptop_rx) =
            penguin_laptop.space::<SecretData>(topic).await.unwrap();
        let (_penguin_mobile_space, _penguin_mobile_rx) =
            penguin_mobile.space::<SecretData>(topic).await.unwrap();

        // Penguin creates a device group.
        let device_group = penguin_laptop
            .create_group(&[(penguin_laptop.id(), AccessLevel::Manage)])
            .await
            .unwrap();

        // Penguin receives the device group event on their system stream.
        loop {
            if let Some(SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            }) = penguin_laptop_system_rx.next().await
            {
                if group_id == device_group.id() {
                    break;
                }
            };
        }

        // Panda receives the device group event on their system stream.
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

        // Panda creates a team group with Penguin's device group as a member.
        let team_group = panda
            .create_group(&[
                (panda.id(), AccessLevel::Manage),
                (device_group.id(), AccessLevel::Write),
            ])
            .await
            .unwrap();
        let team_group_id = team_group.id();

        // Penguin receives the team group event on their system stream.
        loop {
            if let Some(SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            }) = penguin_laptop_system_rx.next().await
            {
                if group_id == team_group.id() {
                    break;
                }
            };
        }

        // Panda receives the team group event on their system stream.
        loop {
            if let Some(SystemEvent::Groups {
                group_id,
                inner: InnerGroupEvent::Created { .. },
                ..
            }) = panda_system_rx.next().await
            {
                if group_id == team_group.id() {
                    break;
                }
            };
        }

        // Panda subscribes to the group event stream.
        let mut panda_team_group_rx = team_group.event_stream();
        // Penguin subscribes to the group event stream.
        let panda_team_group_on_penguin =
            penguin_laptop.group(team_group_id).await.unwrap().unwrap();
        let mut panda_team_group_on_penguin_rx = panda_team_group_on_penguin.event_stream();

        // Penguin adds their mobile to the device group.
        let ready = device_group
            .add(penguin_mobile.id(), AccessLevel::Read)
            .await
            .unwrap();

        ready.await.unwrap();

        // Penguin receives the add event on the team group rx.
        loop {
            if let Some(GroupEvent {
                members,
                actors,
                inner:
                    InnerGroupEvent::Added {
                        group_id: action_group_id,
                        ..
                    },
                ..
            }) = panda_team_group_on_penguin_rx.next().await
            {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Write)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                assert_eq!(actors.len(), 2);
                assert_eq!(action_group_id, device_group.id());
                break;
            };
        }

        // Panda receives the add event on the team group rx.
        loop {
            if let Some(GroupEvent {
                members,
                actors,
                inner:
                    InnerGroupEvent::Added {
                        group_id: action_group_id,
                        ..
                    },
                ..
            }) = panda_team_group_rx.next().await
            {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&(penguin_laptop.id(), AccessLevel::Write)));
                assert!(members.contains(&(penguin_mobile.id(), AccessLevel::Read)));
                assert_eq!(actors.len(), 2);
                assert_eq!(action_group_id, device_group.id());
                break;
            };
        }
    }
}

mod filtered_messages {
    use std::time::Duration;

    use p2panda::streams::StreamEvent;
    use p2panda_core::test_utils::setup_logging;
    use tokio_stream::StreamExt;

    use super::{SecretData, spawn};

    #[tokio::test]
    async fn concurrently_removed_members_filtered() {
        setup_logging();

        use p2panda::Topic;
        use p2panda_auth::AccessLevel;

        let network_id = Topic::random().into();
        let topic = Topic::random();

        let panda = spawn(network_id).await;
        let penguin = spawn(network_id).await;

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

        use p2panda::Topic;
        use p2panda_auth::AccessLevel;

        let network_id = Topic::random().into();
        let topic = Topic::random();

        let panda = spawn(network_id).await;
        let penguin = spawn(network_id).await;
        let tiger = spawn(network_id).await;

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

    use crate::spawn;

    // 1. Node A creates space S with {A, B, C, D} inside
    // 2. Node B removes C from S
    //
    // Node B needs to inform A & D about the new secret after removing C and needs a key bundle of
    // D to do that. If all member logs are correctly associated, B should have received it
    // indirectly via A.
    #[tokio::test]
    async fn indirect_members_log_sync() {
        setup_logging();

        let network_id = Topic::random().into();
        let topic = Topic::random();

        let node_a = spawn(network_id).await;
        let node_b = spawn(network_id).await;
        let node_c = spawn(network_id).await;
        let node_d = spawn(network_id).await;

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
