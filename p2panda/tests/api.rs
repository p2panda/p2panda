// SPDX-License-Identifier: MIT OR Apache-2.0

use futures_util::StreamExt;
use p2panda::streams::EphemeralMessage;
use p2panda_core::{PrivateKey, Topic};
use tokio::task::JoinHandle;

#[tokio::test]
async fn build_and_spawn() -> Result<(), Box<dyn std::error::Error>> {
    // Default & instant setup.
    let _node = p2panda::spawn().await?;

    // Customizable "builder" setup flow.
    let _node = p2panda::builder()
        .database_url("sqlite::memory:")
        .private_key(PrivateKey::new())
        .spawn()
        .await?;

    Ok(())
}

#[tokio::test]
async fn ephemeral_messaging() {
    let chat_id = Topic::new();

    let panda = p2panda::spawn().await.unwrap();
    let icebear = p2panda::spawn().await.unwrap();

    // Panda joins the chat and sends a message to icebear, then waits for an answer.
    let panda_task: JoinHandle<EphemeralMessage<String>> = {
        let chat = panda.ephemeral_stream(chat_id).await.unwrap();

        tokio::spawn(async move {
            chat.publish("Hello, Icebear!".into()).await.unwrap();

            let mut rx = chat.subscribe().await;
            let message = rx.next().await.unwrap();

            message
        })
    };

    // Icebear joins the chat and waits for a message of panda, to then answer.
    let icebear_task: JoinHandle<EphemeralMessage<String>> = {
        let chat = icebear.ephemeral_stream(chat_id).await.unwrap();
        let mut rx = chat.subscribe().await;

        tokio::spawn(async move {
            let message = rx.next().await.unwrap();
            chat.publish("Hi, Panda!".into()).await.unwrap();
            message
        })
    };

    let icebears_received_msg = icebear_task.await.unwrap();
    let pandas_received_msg = panda_task.await.unwrap();

    // Message authors match the senders.
    assert_eq!(icebears_received_msg.author(), panda.id());
    assert_eq!(pandas_received_msg.author(), icebear.id());

    // Everyone received the right messages.
    assert_eq!(icebears_received_msg.body(), &"Hello, Icebear!".to_string());
    assert_eq!(pandas_received_msg.body(), &"Hi, Panda!".to_string());

    // Icebear received the message before panda.
    assert!(icebears_received_msg.timestamp() < pandas_received_msg.timestamp())
}
