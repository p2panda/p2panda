// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::test_utils::setup_logging;

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
    //
    // An empty initial members array will insert us automatically.
    let (_space, _rx) = node.create_space::<SecretData>(topic, &[]).await?;

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
