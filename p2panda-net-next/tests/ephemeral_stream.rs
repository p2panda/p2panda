// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_net_next::NetworkBuilder;
use tokio::sync::broadcast::error::TryRecvError;

// NOTE(glyph): This test will only be meaningful once the address book is fully implemented.
//
// I've included it already to give a demonstration of the external API we're working towards.
#[tokio::test]
async fn two_peer_ephemeral_messaging() {
    let topic_id = [1; 32];

    let join_handle = tokio::spawn(async move {
        let node_builder = NetworkBuilder::new([7; 32]);
        let node = node_builder.build().await.unwrap();

        let stream = node.ephemeral_stream(&topic_id).await.unwrap();

        stream
            .publish(b"I am the nothingness at the centre of creation")
            .await
            .unwrap();

        let mut stream_subscription = stream.subscribe().await.unwrap();

        let msg = stream_subscription.try_recv();

        assert_eq!(msg, Err(TryRecvError::Empty));

        stream.close().unwrap();
    });

    let node_builder = NetworkBuilder::new([7; 32])
        .bind_port_v4(2024)
        .bind_port_v6(2025);
    let node = node_builder.build().await.unwrap();

    let stream = node.ephemeral_stream(&topic_id).await.unwrap();

    stream.publish(b"((( )))").await.unwrap();

    let mut stream_subscription = stream.subscribe().await.unwrap();

    let msg = stream_subscription.try_recv();

    assert_eq!(msg, Err(TryRecvError::Empty));

    stream.close().unwrap();

    join_handle.await.unwrap();
}
