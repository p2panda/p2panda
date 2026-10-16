// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda::{NetworkId, Node};

async fn spawn_node(network_id: NetworkId) -> Node {
    p2panda::builder()
        .network_id(network_id)
        .spawn()
        .await
        .unwrap()
}

mod groups_processor {
    use p2panda::{AccessLevel, Hash, Topic};
    use p2panda_core::test_utils::setup_logging;
    use p2panda_spaces::AuthGroupState;
    use p2panda_store::{groups::GroupsStore, tx_unwrap};

    use crate::spawn_node;

    #[tokio::test]
    async fn namespaced_states() {
        setup_logging();

        let network_id = Topic::random().into();
        let panda = spawn_node(network_id).await;

        // The "create" group message is consumed by the groups processor.
        let (group, _rx) = panda
            .create_group(&[(panda.id(), AccessLevel::Manage)])
            .await
            .unwrap();
        let store = panda.store();

        // We expect that state is namespaced by the group id.
        let group_retrieved: Option<AuthGroupState<()>> = tx_unwrap!(
            store,
            store
                .get_groups_state_tx(Hash::from_bytes(*group.id().as_bytes()))
                .await
        )
        .unwrap();

        assert!(group_retrieved.is_some());
        let group_retrieved = group_retrieved.unwrap();

        assert!(group_retrieved.has_group(group.id()));
    }
}

mod group_events {
    use p2panda::Topic;
    use p2panda::spaces::InnerGroupEvent;
    use p2panda::streams::{GroupAction, StreamEvent, SystemEvent};
    use p2panda_auth::AccessLevel;
    use p2panda_core::test_utils::setup_logging;
    use tokio_stream::StreamExt;

    use super::spawn_node;

    #[tokio::test]
    async fn group_system_events() {
        setup_logging();

        let network_id = Topic::random().into();

        let panda = spawn_node(network_id).await;
        let penguin = spawn_node(network_id).await;

        let mut panda_system_rx = panda.event_stream().await.unwrap();
        let mut penguin_system_rx = penguin.event_stream().await.unwrap();

        // Penguin creates a device group.
        let (device_group, _) = penguin
            .create_group(&[(penguin.id(), AccessLevel::Manage)])
            .await
            .unwrap();

        // Panda subscribes to the group.
        let (_device_group_on_panda, _) = panda.group(device_group.id()).await.unwrap();

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
    }

    #[tokio::test]
    async fn group_events() {
        setup_logging();

        let network_id = Topic::random().into();

        let penguin_laptop = spawn_node(network_id).await;
        let penguin_mobile = spawn_node(network_id).await;

        // Penguin creates a device group on laptop.
        let (device_group, mut device_group_rx) = penguin_laptop
            .create_group(&[(penguin_laptop.id(), AccessLevel::Manage)])
            .await
            .unwrap();

        // Penguin subscribes to device group on mobile.
        let (_device_group_mobile, mut device_group_mobile_rx) =
            penguin_mobile.group(device_group.id()).await.unwrap();

        // Penguin receives the device group "create" event on their laptop.
        loop {
            if let Some(StreamEvent::Group {
                group_id, action, ..
            }) = device_group_rx.next().await
            {
                if group_id == device_group.id() && matches!(action, GroupAction::Created { .. }) {
                    break;
                }
            };
        }

        // Penguin receives the device group "create" event on their mobile.
        loop {
            if let Some(StreamEvent::Group {
                group_id, action, ..
            }) = device_group_mobile_rx.next().await
            {
                if group_id == device_group.id() && matches!(action, GroupAction::Created { .. }) {
                    break;
                }
            };
        }

        // Penguin mobile added to group.
        device_group
            .add(penguin_mobile.id(), AccessLevel::Write)
            .await
            .unwrap();

        // Penguin receives the device group "add" event on their laptop.
        loop {
            if let Some(StreamEvent::Group {
                group_id, action, ..
            }) = device_group_rx.next().await
            {
                if group_id == device_group.id() && matches!(action, GroupAction::Added { .. }) {
                    break;
                }
            };
        }

        // Penguin receives the device group "add" event on their mobile.
        loop {
            if let Some(StreamEvent::Group {
                group_id, action, ..
            }) = device_group_mobile_rx.next().await
            {
                if group_id == device_group.id() && matches!(action, GroupAction::Added { .. }) {
                    break;
                }
            };
        }
    }
}

mod nested_groups {
    use futures_util::StreamExt;
    use p2panda::streams::{GroupAction, StreamEvent};
    use p2panda::{AccessLevel, Topic};
    use p2panda_core::test_utils::setup_logging;

    use crate::spawn_node;

    #[tokio::test]
    async fn team_with_device_groups() {
        setup_logging();

        let network_id = Topic::random().into();

        let penguin_laptop = spawn_node(network_id).await;
        let penguin_mobile = spawn_node(network_id).await;

        // Penguin creates a device group on laptop.
        let (device_group, _device_group_rx) = penguin_laptop
            .create_group(&[
                (penguin_laptop.id(), AccessLevel::Manage),
                (penguin_mobile.id(), AccessLevel::Manage),
            ])
            .await
            .unwrap();

        // Penguin creates a team group on laptop with the device group as a member.
        let (team_group, _team_group_rx) = penguin_laptop
            .create_group(&[(device_group.id(), AccessLevel::Write)])
            .await
            .unwrap();

        // Penguin subscribes to the team group on their mobile.
        let (_team_group_mobile, mut team_group_mobile_rx) =
            penguin_mobile.group(team_group.id()).await.unwrap();

        // Penguin receives the device group "create" event on their mobile.
        loop {
            if let Some(StreamEvent::Group {
                group_id, action, ..
            }) = team_group_mobile_rx.next().await
            {
                if group_id == device_group.id() && matches!(action, GroupAction::Created { .. }) {
                    break;
                }
            };
        }

        // Penguin receives the team group "create" event on their mobile.
        loop {
            if let Some(StreamEvent::Group {
                group_id, action, ..
            }) = team_group_mobile_rx.next().await
            {
                if group_id == team_group.id() && matches!(action, GroupAction::Created { .. }) {
                    break;
                }
            };
        }
    }

    #[tokio::test]
    async fn team_with_device_groups_live_mode() {
        setup_logging();

        let network_id = Topic::random().into();

        let penguin_laptop = spawn_node(network_id).await;
        let penguin_mobile = spawn_node(network_id).await;

        // Penguin creates a device group on laptop.
        let (device_group, _device_group_rx) = penguin_laptop
            .create_group(&[
                (penguin_laptop.id(), AccessLevel::Manage),
                (penguin_mobile.id(), AccessLevel::Manage),
            ])
            .await
            .unwrap();

        // Penguin creates a team group on laptop with only themselves as a member for now.
        let (team_group, _team_group_rx) = penguin_laptop
            .create_group(&[(penguin_laptop.id(), AccessLevel::Manage)])
            .await
            .unwrap();

        // Penguin subscribes to the team group on their mobile.
        let (_team_group_mobile, mut team_group_mobile_rx) =
            penguin_mobile.group(team_group.id()).await.unwrap();

        // Penguin adds the device group to the team.
        //
        // NOTE: both peers are already subscribed to the team group so we can verify that both
        // team group operations _and_ required device group operations are sent in live mode.
        team_group
            .add(device_group.id(), AccessLevel::Write)
            .await
            .unwrap();

        let mut team_group_create_received = false;
        let mut device_group_create_received = false;
        let mut team_group_add_received = false;

        // Penguin receives the device group "create" event on their mobile.
        loop {
            if let Some(StreamEvent::Group {
                group_id, action, ..
            }) = team_group_mobile_rx.next().await
            {
                if group_id == team_group.id() && matches!(action, GroupAction::Created { .. }) {
                    team_group_create_received = true;
                }

                if group_id == device_group.id() && matches!(action, GroupAction::Created { .. }) {
                    device_group_create_received = true;
                }

                if group_id == team_group.id()
                    && matches!(action, GroupAction::Added { actor, .. } if actor.id() == device_group.id())
                {
                    team_group_add_received = true;
                }

                if team_group_create_received
                    && device_group_create_received
                    && team_group_add_received
                {
                    break;
                }
            };
        }
    }
}

mod groups_api_validation {
    use std::assert_matches;

    use p2panda::spaces::{AddGroupMemberError, RemoveGroupMemberError};
    use p2panda::{SigningKey, Topic};
    use p2panda_auth::AccessLevel;
    use p2panda_auth::validation::{AddMemberError, RemoveMemberError};
    use p2panda_core::test_utils::setup_logging;

    use super::spawn_node;

    #[tokio::test]
    async fn api_validation() {
        setup_logging();

        let network_id = Topic::random().into();

        let panda = spawn_node(network_id).await;
        let lion = spawn_node(network_id).await;
        let tiger = spawn_node(network_id).await;

        let (panda_group, _) = panda
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

        // Lion subscribes and receives the group.
        let (lion_group, _lion_group_rx) = lion.group(panda_group.id()).await.unwrap();
        let lion_group = lion_group.created().await.unwrap();

        // Tiger subscribes and receives the group.
        let (tiger_group, _tiger_group_rx) = tiger.group(panda_group.id()).await.unwrap();
        let tiger_group = tiger_group.created().await.unwrap();

        // Tiger isn't a recognized group actor.
        let result = tiger_group.add(tiger.id(), AccessLevel::Write).await;
        assert_matches!(
            result.err().unwrap(),
            AddGroupMemberError::Validation {
                err: AddMemberError::UnrecognisedActor,
                ..
            }
        );

        // Lion doesn't have required access level.
        let result = lion_group.remove(panda.id()).await;
        assert_matches!(
            result.err().unwrap(),
            RemoveGroupMemberError::Validation {
                err: RemoveMemberError::InsufficientAccess,
                ..
            }
        );
    }
}

// TODO: test that group events are emitted on replays (they should be as they are not
// carrying enriched data).
