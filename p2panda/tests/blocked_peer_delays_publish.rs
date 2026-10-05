// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::{Duration, Instant};

use p2panda::streams::{StreamEvent, StreamPublisher, StreamSubscription};
use p2panda::{NetworkId, Node, Topic};
use p2panda_core::test_utils::setup_logging;
use tokio_stream::StreamExt;

async fn spawn_node(network_id: NetworkId) -> Node {
    p2panda::builder()
        .network_id(network_id)
        .spawn()
        .await
        .unwrap()
}

async fn receive(rx: &mut StreamSubscription<String>, expected: &str) {
    let wait = async {
        while let Some(event) = rx.next().await {
            if let StreamEvent::Processed { operation, .. } = event
                && operation.message() == expected
            {
                return;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(60), wait)
        .await
        .unwrap_or_else(|_| panic!("{expected:?} never arrived"));
}

/// Publishes a message every second and asserts each one arrives within a second.
async fn expect_prompt_delivery(
    tx: &StreamPublisher<String>,
    rx: &mut StreamSubscription<String>,
    phase: &str,
) {
    for i in 1..=5 {
        let message = format!("{phase} {i}");
        let published = Instant::now();
        tx.publish(message.clone()).await.unwrap();
        receive(rx, &message).await;
        let latency = published.elapsed();
        assert!(
            latency < Duration::from_secs(1),
            "penguin received {message:?} only after {latency:?}"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[tokio::test]
async fn blocked_peer_does_not_delay_publishing_to_other_peers() {
    setup_logging();

    let topic = Topic::random();
    let network_id: NetworkId = Topic::random().into();

    let panda = spawn_node(network_id).await;
    let icebear = spawn_node(network_id).await;
    let penguin = spawn_node(network_id).await;

    let (panda_tx, _panda_rx) = panda.stream::<String>(topic).await.unwrap();
    let (_penguin_tx, mut penguin_rx) = penguin.stream::<String>(topic).await.unwrap();

    // Let panda and penguin discover each other and establish their live sync session.
    panda_tx.publish("hello".into()).await.unwrap();
    receive(&mut penguin_rx, "hello").await;

    // Control: with nobody rejecting panda, messages reach penguin promptly.
    expect_prompt_delivery(&panda_tx, &mut penguin_rx, "before block").await;

    // Icebear joins the topic but rejects panda, so every sync session panda initiates towards
    // icebear fails and panda keeps retrying it every 5 seconds.
    icebear.sync_block_list().block(panda.id()).await;
    let (_icebear_tx, _icebear_rx) = icebear.stream::<String>(topic).await.unwrap();

    expect_prompt_delivery(&panda_tx, &mut penguin_rx, "after block").await;
}
