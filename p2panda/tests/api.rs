// SPDX-License-Identifier: MIT OR Apache-2.0

// TODO: Remove this later.
#![allow(unused)]

use futures_core::Stream;
use p2panda::Topic;
use p2panda_core::PrivateKey;

#[tokio::test]
async fn it_works() -> Result<(), Box<dyn std::error::Error>> {
    let node = p2panda::spawn().await?;
    println!("{}", node.id());

    let node = p2panda::builder()
        .database_url("sqlite::memory:")
        .private_key(PrivateKey::new())
        .spawn()
        .await?;
    println!("{}", node.id());

    // TODO: All of this is unimplemented:
    // let channel_id = Topic::new();
    // let channel = node.ephemeral_stream(channel_id).await?;
    // println!("{}", channel_id);
    //
    // channel.publish("Hello, Panda!".to_string()).await?;
    //
    // let rx = channel.subscribe().await?;
    // while let Some(event) = rx.next().await {}

    Ok(())
}
