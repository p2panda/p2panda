// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda::Topic;
use p2panda::streams::{PublishError, StreamPublisher};
use p2panda_core::test_utils::setup_logging;

#[tokio::test]
async fn two_nodes_publish_on_one_database_file() {
    setup_logging();

    let path = std::env::temp_dir().join(format!(
        "p2panda-shared-{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let url = format!("sqlite://{}", path.display());
    let network_id = Topic::random().into();

    let node_a = p2panda::builder()
        .network_id(network_id)
        .database_url(&url)
        .spawn()
        .await
        .unwrap();
    let node_b = p2panda::builder()
        .network_id(network_id)
        .database_url(&url)
        .spawn()
        .await
        .unwrap();

    let (tx_a, _rx_a) = node_a.stream::<String>(Topic::random()).await.unwrap();
    let (tx_b, _rx_b) = node_b.stream::<String>(Topic::random()).await.unwrap();

    let publish_many = |tx: StreamPublisher<String>| async move {
        for i in 0..100 {
            tx.publish(format!("message {i}")).await?;
        }
        Ok::<_, PublishError>(())
    };

    let a = tokio::spawn(publish_many(tx_a));
    let b = tokio::spawn(publish_many(tx_b));

    a.await
        .unwrap()
        .expect("node A publishes while node B does");
    b.await
        .unwrap()
        .expect("node B publishes while node A does");

    let _ = std::fs::remove_file(&path);
}
