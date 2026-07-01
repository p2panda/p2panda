// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use p2panda::{operation::Header, spaces::SpaceEvent};
use p2panda_auth::{Access, AccessLevel};
use p2panda_core::{cbor::decode_cbor, test_utils::setup_logging};
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

    // We can manage (nested) groups (useful for multi-device, etc.)
    let penguin_laptop = p2panda::spawn().await?;
    let penguin_mobile = p2panda::spawn().await?;

    {
        // Penguin subscribes to the space in order to publish some key bundles.
        let (_, _penguin_laptop_rx) = penguin_laptop.space::<SecretData>(topic).await.unwrap();
        let (_, _penguin_mobile_rx) = penguin_mobile.space::<SecretData>(topic).await.unwrap();

        // Wait some time for key bundle, groups and space logs to sync.
        // @TODO: replace sleep when spaces events are watchable.
        sleep(Duration::from_secs(4)).await;
    }

    // @TODO: currently these group messages are _not_ sent via live-mode for the spaces topic.
    // Need to consider how best to get access to the sync handle in the node API to make this
    // possible.
    let penguin = panda
        .create_group(&[
            (penguin_laptop.id(), AccessLevel::Read),
            (penguin_mobile.id(), AccessLevel::Write),
        ])
        .await?;

    // .. and add them to the space as well.
    let ready = panda_space.add(penguin, AccessLevel::Read).await?;
    ready.await?;

    // @TODO: Penguins subscribe to the space topic again in order to get the groups messages via
    // sync.
    let (penguin_laptop_space, mut penguin_laptop_rx) =
        penguin_laptop.space::<SecretData>(topic).await.unwrap();
    let (penguin_mobile_space, mut penguin_mobile_rx) =
        penguin_mobile.space::<SecretData>(topic).await.unwrap();

    sleep(Duration::from_secs(3)).await;

    println!("assert space members");

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

    println!("await panda message");
    // Panda receives the message they sent.
    loop {
        let Some(event) = panda_rx.next().await else {
            panic!("unexpected stream closure");
        };
        let SpaceEvent::Processed { operation, .. } = event else {
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
        let SpaceEvent::Processed { operation, .. } = event else {
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
        let SpaceEvent::Processed { operation, .. } = event else {
            continue;
        };
        assert_eq!(&message, operation.message());
        break;
    }

    Ok(())
}

#[tokio::test]
async fn spaces_sync() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    use p2panda::Topic;
    use p2panda_auth::AccessLevel;

    let topic = Topic::random();

    let panda = p2panda::spawn().await?;
    let penguin = p2panda::spawn().await?;

    // Penguin subscribes to the space (and publishes a key bundle).
    let (_penguin_space, mut penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();

    // Panda creates and subscribes to a space.
    let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await?;

    // Wait some time for key bundle, groups and space logs to sync.
    // @TODO: replace sleep when spaces events are watchable.
    sleep(Duration::from_secs(2)).await;

    // Panda adds penguin as a member of the space.
    //
    // They can do this because they received their key bundle by now.
    let ready = panda_space.add(penguin.id(), AccessLevel::Read).await?;
    assert!(ready.await.is_ok());

    // Panda publishes a message to all members.
    let message = SecretData {
        title: "My favorite things".to_string(),
        content: "Hello, everyone!".to_string(),
    };

    sleep(Duration::from_secs(2)).await;
    let ready = panda_space.publish(message.clone()).await?;
    assert!(ready.await.is_ok());

    sleep(Duration::from_secs(1)).await;

    // Panda receives the message they sent.
    println!("await panda receive");
    loop {
        let Some(event) = panda_rx.next().await else {
            panic!("unexpected stream closure");
        };
        let SpaceEvent::Processed { operation, .. } = event else {
            continue;
        };
        assert_eq!(&message, operation.message());
        break;
    }

    // penguin also receives the message.
    // @TODO: currently fails because of operation decoding bug.
    println!("await penguin receive");
    loop {
        let Some(event) = penguin_rx.next().await else {
            panic!("unexpected stream closure");
        };
        let SpaceEvent::Processed { operation, .. } = event else {
            continue;
        };
        assert_eq!(&message, operation.message());
        break;
    }

    // @TODO: assert that penguin has also successfully joined the space and can publish a
    // message. This would often fail now due to ordering not yet being implemented.

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
    let (_, _, _, space_message) = space.add(penguin.id(), Access::read()).await?;

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

    use p2panda::Topic;
    use p2panda_auth::AccessLevel;

    let topic = Topic::random();

    let panda = p2panda::spawn().await?;
    let penguin = p2panda::spawn().await?;

    // Penguin creates a group before subscribing to the space.
    let penguin_group = penguin
        .create_group(&[(penguin.id(), AccessLevel::Manage)])
        .await?;

    // They then subscribe, as does panda.
    let (_penguin_space, _penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();
    let (panda_space, _panda_rx) = panda.create_space::<SecretData>(topic).await?;
    sleep(Duration::from_secs(5)).await;

    // We expect panda to be able to add penguin group as a space member now.
    let ready = panda_space
        .add(penguin_group.id(), AccessLevel::Read)
        .await?;
    assert!(ready.await.is_ok());

    Ok(())
}

#[tokio::test]
async fn live_repair_space() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    use p2panda::Topic;
    use p2panda_auth::AccessLevel;

    let topic = Topic::random();

    let panda = p2panda::spawn().await?;
    let penguin = p2panda::spawn().await?;

    // Penguin subscribes to the space.
    let (_penguin_space, _penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();
    let (panda_space, _panda_rx) = panda.create_space::<SecretData>(topic).await?;
    sleep(Duration::from_secs(3)).await;

    // And then creates a group.
    let penguin_group = penguin
        .create_group(&[(penguin.id(), AccessLevel::Manage)])
        .await?;

    sleep(Duration::from_secs(3)).await;

    // We expect panda to be able to add penguin group to the space.
    let ready = panda_space
        .add(penguin_group.id(), AccessLevel::Read)
        .await?;
    assert!(ready.await.is_ok());

    Ok(())
}
