// SPDX-License-Identifier: MIT OR Apache-2.0

use std::assert_matches;
use std::collections::HashSet;

use p2panda::Topic;
use p2panda::spaces::{
    AddSpaceMemberError, GroupEvent, InnerGroupEvent, PublishSpaceError, RemoveSpaceMemberError,
};
use p2panda::streams::{StreamEvent, SystemEvent};
use p2panda::{SigningKey, operation::Header};
use p2panda_auth::validation::{AddMemberError, RemoveMemberError, WriteError};
use p2panda_auth::{Access, AccessLevel};
use p2panda_core::{cbor::decode_cbor, test_utils::setup_logging};
use p2panda_spaces::SpaceEvent;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SecretData {
    title: String,
    content: String,
}

#[tokio::test]
async fn spaces_api() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    use p2panda::Topic;

    let panda = p2panda::spawn().await?;
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
    let penguin_laptop = p2panda::spawn().await?;
    let penguin_mobile = p2panda::spawn().await?;
    let mut penguin_mobile_system_rx = penguin_mobile.event_stream().await?;

    // Penguin subscribes to the space in order to publish some key bundles.
    let (penguin_laptop_space, mut penguin_laptop_rx) =
        penguin_laptop.space::<SecretData>(topic).await.unwrap();
    let (penguin_mobile_space, mut penguin_mobile_rx) =
        penguin_mobile.space::<SecretData>(topic).await.unwrap();

    // Panda receives both penguins key bundles.
    let mut expected = HashSet::from([penguin_laptop.id(), penguin_mobile.id()]);
    while let Some(event) = panda_rx.next().await {
        if let StreamEvent::KeyBundle(verifying_key) = event {
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
        if let SystemEvent::Auth(_) = event {
            break;
        };
    }

    // Penguin mobile receives the group.
    while let Some(event) = penguin_mobile_system_rx.next().await {
        if let SystemEvent::Auth(_) = event {
            break;
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

    let topic = Topic::random();

    let panda = p2panda::spawn().await?;
    let penguin = p2panda::spawn().await?;

    // Penguin subscribes to the space (and publishes a key bundle).
    let (_penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();

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
        if let StreamEvent::KeyBundle(..) = event {
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

#[tokio::test]
async fn encode_decode() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    use p2panda::Topic;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct SecretData {
        title: String,
        content: String,
    }

    let topic = Topic::random();

    let panda = p2panda::spawn().await?;
    let penguin = p2panda::spawn().await?;
    let member = penguin.me().await?;
    panda.register_member(member).await?;

    let (space, _panda_rx) = panda.create_space::<SecretData>(topic).await?;

    // Access the inner spaces manager so we can directly create and access an add message.
    let spaces_manager = panda.spaces_manager();
    let space = spaces_manager.space(space.id()).await?.unwrap();
    let (_, _, _, space_message, _) = space.add(penguin.id(), Access::read()).await?;

    // Encode and decode the add operation.
    let operation = space_message.into_operation();
    let header = operation.header();
    let cbor_bytes = header.to_bytes();
    let decoded_header: Header = decode_cbor(&cbor_bytes[..]).unwrap();

    // This fails half the time.
    assert_eq!(header.hash(), decoded_header.hash());

    Ok(())
}

#[tokio::test]
async fn sync_repair_space() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    let topic = Topic::random();

    let panda = p2panda::spawn().await?;
    let mut panda_system_rx = panda.event_stream().await?;
    let penguin = p2panda::spawn().await?;
    let mut penguin_system_rx = penguin.event_stream().await?;

    // Penguin creates a group before subscribing to the space.
    let penguin_group = penguin
        .create_group(&[(penguin.id(), AccessLevel::Manage)])
        .await?;

    // They then subscribe, as does panda.
    let (_penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();
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

    // Panda receives the group.
    while let Some(event) = panda_system_rx.next().await {
        if let SystemEvent::Auth(GroupEvent {
            group_id,
            inner: InnerGroupEvent::Created { .. },
            ..
        }) = event
        {
            assert_eq!(group_id, penguin_group.id());
            break;
        };
    }

    // Penguin receives the group.
    while let Some(event) = penguin_system_rx.next().await {
        if let SystemEvent::Auth(GroupEvent {
            group_id,
            inner: InnerGroupEvent::Created { .. },
            ..
        }) = event
        {
            assert_eq!(group_id, penguin_group.id());
            break;
        };
    }
    // We expect panda to be able to add penguin group as a space member now.
    panda_space
        .add(penguin_group.id(), AccessLevel::Read)
        .await?;

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

    Ok(())
}

#[tokio::test]
async fn live_repair_space() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    let topic = Topic::random();

    let panda = p2panda::spawn().await?;
    let mut panda_system_rx = panda.event_stream().await?;
    let penguin = p2panda::spawn().await?;

    // Penguin subscribes to the space.
    let (_penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();
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

    // And then creates a group.
    let penguin_group = penguin
        .create_group(&[(penguin.id(), AccessLevel::Manage)])
        .await?;

    // Panda receives the group.
    while let Some(event) = panda_system_rx.next().await {
        if let SystemEvent::Auth(GroupEvent {
            group_id,
            inner: InnerGroupEvent::Created { .. },
            ..
        }) = event
        {
            assert_eq!(group_id, penguin_group.id());
            break;
        };
    }

    // We expect panda to be able to add penguin group to the space.
    panda_space
        .add(penguin_group.id(), AccessLevel::Read)
        .await?;

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
    Ok(())
}

#[tokio::test]
async fn api_validation() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    let topic = Topic::random();

    let panda = p2panda::spawn().await?;

    let (panda_space, mut panda_rx) = panda.create_space::<String>(topic).await?;

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
    let tiger = p2panda::spawn().await?;
    let (tiger_space, mut tiger_rx) = tiger.space::<String>(topic).await?;

    while let Some(event) = panda_rx.next().await {
        if let StreamEvent::KeyBundle(verifying_key) = event {
            if verifying_key == tiger.id() {
                break;
            }
        };
    }

    // Panda adds tiger with read-only access.
    panda_space.add(tiger.id(), AccessLevel::Read).await?;

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
    panda_space.remove(panda.id()).await?;

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

    Ok(())
}
