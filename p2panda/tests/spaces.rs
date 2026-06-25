// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use p2panda::{operation::Header, spaces::SpaceEvent};
use p2panda_auth::Access;
use p2panda_core::{cbor::decode_cbor, test_utils::setup_logging};
use tokio::time::sleep;
use tokio_stream::StreamExt;

#[tokio::test]
async fn spaces_api() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    // use p2panda::SigningKey;
    // use p2panda_auth::AccessLevel;
    use p2panda::Topic;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct SecretData {
        title: String,
        content: String,
    }

    let node = p2panda::spawn().await?;

    // Spaces behave like topic-streams, just that they're encrypted towards members.
    let topic = Topic::random();

    // Create a space with only us inside.
    let (_space, _rx) = node.create_space::<SecretData>(topic).await?;

    // TODO
    // We can manage (nested) groups (useful for multi-device, etc.)
    // let penguin_laptop = SigningKey::generate().verifying_key();
    // let penguin_mobile = SigningKey::generate().verifying_key();
    //
    // let penguin = node
    //     .create_group(&[
    //         (penguin_laptop, AccessLevel::Read),
    //         (penguin_mobile, AccessLevel::Write),
    //     ])
    //     .await?;
    //
    // // .. and add them to the space as well.
    // space.add(penguin, AccessLevel::Read).await?;
    //
    // // Every message published into a space can be decrypted by it's members.
    // space
    //     .publish(SecretData {
    //         title: "My favorite things".to_string(),
    //         content: "Hello, everyone!".to_string(),
    //     })
    //     .await?;

    Ok(())
}

#[tokio::test]
async fn spaces_sync() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    use p2panda::Topic;
    use p2panda_auth::AccessLevel;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct SecretData {
        title: String,
        content: String,
    }

    let topic = Topic::random();

    let panda = p2panda::spawn().await?;
    let penguin = p2panda::spawn().await?;

    // Penguin subscribes to the space (and publishes a key bundle).
    let (_penguin_space, _penguin_rx) = penguin.space::<SecretData>(topic).await.unwrap();

    // Panda creates and subscribes to a space.
    let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await?;

    // Wait some time for key bundle, groups and space logs to sync.
    // @TODO: replace sleep when spaces events are watchable.
    sleep(Duration::from_secs(4)).await;

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
    let ready = panda_space.publish(message.clone()).await?;
    assert!(ready.await.is_ok());

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

    // penguin also receives the message.
    // @TODO: currently fails because of lack of ordering and operation decoding bug.
    // loop {
    //     let Some(event) = penguin_rx.next().await else {
    //         panic!("unexpected stream closure");
    //     };
    //     let SpaceEvent::Processed { operation, .. } = event else {
    //         continue;
    //     };
    //     assert_eq!(&message, operation.message());
    //     break;
    // }

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
