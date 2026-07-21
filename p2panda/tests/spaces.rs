// SPDX-License-Identifier: MIT OR Apache-2.0

use std::assert_matches;
use std::collections::HashSet;
use std::time::Duration;

use p2panda::Topic;
use p2panda::spaces::{AddSpaceMemberError, PublishSpaceError, RemoveSpaceMemberError};
use p2panda::streams::StreamEvent;
use p2panda::{SigningKey, operation::Header};
use p2panda_auth::validation::{AddMemberError, RemoveMemberError, WriteError};
use p2panda_auth::{Access, AccessLevel};
use p2panda_core::{cbor::decode_cbor, test_utils::setup_logging};
use p2panda_spaces::SpaceEvent;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
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

    // Spaces behave like topic-streams, just that they're encrypted towards members.
    let topic = Topic::random();

    // Create a space with only us inside.
    let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await?;

    // Panda receives a space created event for their own action.
    while let Some(event) = panda_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Created { .. }) = event {
            break;
        };
    }

    // We can manage (nested) groups (useful for multi-device, etc.)
    let penguin_laptop = p2panda::spawn().await?;
    let penguin_mobile = p2panda::spawn().await?;

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
    // @TODO: Switch to observing groups events when this is implemented.
    loop {
        let group = panda.group(penguin.id()).await?;
        if group.is_some() {
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }

    // Penguin mobile receives the group.
    // @TODO: Switch to observing groups events when this is implemented.
    loop {
        let group = penguin_mobile.group(penguin.id()).await?;
        if group.is_some() {
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }

    panda_space.add(penguin.id(), AccessLevel::Read).await?;

    while let Some(event) = panda_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Added { added, .. }) = event {
            assert_eq!(added.len(), 2);
            assert!(added.contains(&(penguin_laptop.id(), Access::read())));
            assert!(added.contains(&(penguin_mobile.id(), Access::read())));
            break;
        };
    }

    while let Some(event) = penguin_laptop_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Added { added, .. }) = event {
            assert_eq!(added.len(), 2);
            assert!(added.contains(&(penguin_laptop.id(), Access::read())));
            assert!(added.contains(&(penguin_mobile.id(), Access::read())));
            break;
        };
    }

    while let Some(event) = penguin_mobile_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Added { added, .. }) = event {
            assert_eq!(added.len(), 2);
            assert!(added.contains(&(penguin_laptop.id(), Access::read())));
            assert!(added.contains(&(penguin_mobile.id(), Access::read())));
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

    while let Some(event) = panda_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Promoted { promoted, .. }) = event {
            assert_eq!(promoted.len(), 1);
            // NOTE: Penguin mobile is not promoted as they don't hold "write" access in the sub-group.
            assert!(promoted.contains(&(penguin_laptop.id(), Access::write())));
            break;
        };
    }

    assert!(
        panda_space
            .actors()
            .await?
            .contains(&(penguin.id(), AccessLevel::Write))
    );

    // Panda demotes penguin to have "read" access.
    assert!(
        panda_space
            .demote(penguin.id(), AccessLevel::Read)
            .await
            .is_ok()
    );

    while let Some(event) = panda_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Demoted { demoted, .. }) = event {
            assert_eq!(demoted.len(), 1);
            assert!(demoted.contains(&(penguin_laptop.id(), Access::read())));
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
        if let StreamEvent::Space(SpaceEvent::Promoted { promoted, .. }) = event {
            assert_eq!(promoted.len(), 1);
            assert!(promoted.contains(&(penguin_laptop.id(), Access::write())));
            break;
        };
    }

    while let Some(event) = penguin_laptop_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Demoted { demoted, .. }) = event {
            assert_eq!(demoted.len(), 1);
            assert!(demoted.contains(&(penguin_laptop.id(), Access::read())));
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
        if let StreamEvent::Space(SpaceEvent::Created { .. }) = event {
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
        if let StreamEvent::Space(SpaceEvent::Added { added, .. }) = event {
            assert_eq!(added.len(), 1);
            assert!(added.contains(&(penguin.id(), Access::read())));
            break;
        };
    }

    while let Some(event) = penguin_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Added { added, .. }) = event {
            assert_eq!(added.len(), 1);
            assert!(added.contains(&(penguin.id(), Access::read())));
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
    let penguin = p2panda::spawn().await?;

    // Penguin creates a group before subscribing to the space.
    let penguin_group = penguin
        .create_group(&[(penguin.id(), AccessLevel::Manage)])
        .await?;

    // They then subscribe, as does panda.
    let (_penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();
    let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await?;

    while let Some(event) = panda_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Created { .. }) = event {
            break;
        };
    }

    // Panda materialized the group.
    // @TODO: Switch to observing groups events when this is implemented.
    loop {
        let group = panda.group(penguin_group.id()).await?;
        if group.is_some() {
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }

    // Penguin mobile materialized the group.
    // @TODO: Switch to observing groups events when this is implemented.
    loop {
        let group = penguin.group(penguin_group.id()).await?;
        if group.is_some() {
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }

    // We expect panda to be able to add penguin group as a space member now.
    panda_space
        .add(penguin_group.id(), AccessLevel::Read)
        .await?;

    while let Some(event) = panda_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Added { added, .. }) = event {
            assert_eq!(added.len(), 1);
            assert!(added.contains(&(penguin.id(), Access::read())));
            break;
        };
    }

    while let Some(event) = penguin_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Added { added, .. }) = event {
            assert_eq!(added.len(), 1);
            assert!(added.contains(&(penguin.id(), Access::read())));
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
    let penguin = p2panda::spawn().await?;

    // Penguin subscribes to the space.
    let (_penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();
    let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await?;

    while let Some(event) = panda_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Created { .. }) = event {
            break;
        };
    }

    // And then creates a group.
    let penguin_group = penguin
        .create_group(&[(penguin.id(), AccessLevel::Manage)])
        .await?;

    // Panda materialized the group.
    // @TODO: Switch to observing groups events when this is implemented.
    loop {
        let group = panda.group(penguin_group.id()).await?;
        if group.is_some() {
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }

    // We expect panda to be able to add penguin group to the space.
    panda_space
        .add(penguin_group.id(), AccessLevel::Read)
        .await?;

    while let Some(event) = panda_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Added { added, .. }) = event {
            assert_eq!(added.len(), 1);
            assert!(added.contains(&(penguin.id(), Access::read())));
            break;
        };
    }

    while let Some(event) = penguin_rx.next().await {
        if let StreamEvent::Space(SpaceEvent::Added { added, .. }) = event {
            assert_eq!(added.len(), 1);
            assert!(added.contains(&(penguin.id(), Access::read())));
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
        if let StreamEvent::Space(SpaceEvent::Added { .. }) = event {
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
