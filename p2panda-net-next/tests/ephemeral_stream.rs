use p2panda_net_next::NetworkBuilder;
use tokio::sync::broadcast::error::TryRecvError;

// NOTE(glyph): This test will only be meaningful once the address book is fully implemented.
//
// I've included it already to give a demonstration of the external API we're working towards.
#[tokio::test]
async fn two_peer_ephemeral_messaging() {
    let topic_id = [1; 32];

    let node_a_builder = NetworkBuilder::new([7; 32]);
    //let node_b_builder = NetworkBuilder::new([7; 32])
    //    .bind_port_v4(2024)
    //    .bind_port_v6(2025);

    let node_a = node_a_builder.build().await.unwrap();
    //let node_b = node_b_builder.build().await.unwrap();

    let stream_a = node_a.ephemeral_stream(&topic_id).await.unwrap();
    //let stream_b = node_b.ephemeral_stream(&topic_id).await.unwrap();

    stream_a
        .publish(b"I am the nothingness at the centre of creation")
        .await
        .unwrap();

    let mut stream_a_subscription = stream_a.subscribe().await.unwrap();

    let msg = stream_a_subscription.try_recv();

    assert_eq!(msg, Err(TryRecvError::Empty));
}
