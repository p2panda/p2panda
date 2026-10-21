// SPDX-License-Identifier: MIT OR Apache-2.0

use futures_util::Stream;
use p2panda_core::{Body, SigningKey, Topic};
use tokio_stream::StreamExt;

use crate::egress::{
    DispatchConfig, Egress, EventDeliveryPolicy, EventProcessingPolicy, StreamType,
};
use crate::operation::{Extensions, Header, Operation};

#[tokio::test]
async fn dispatch_via_egress() {
    let topic = Topic::random();
    let egress = Egress::new();

    let mut rx = egress
        .register_processing(topic, StreamType::Topic)
        .unwrap();

    let handle = egress.handle();

    let operation = {
        let signing_key = SigningKey::generate();
        let body = Body::from_bytes(b"Hello!");
        let header = Header::builder()
            .body(&body)
            .build(&signing_key, Extensions::from_topic(topic));
        Operation::from_parts(header, Some(body))
    };

    let event = tokio::spawn(async move {
        let (event, result_tx) = rx.next().await.expect("event arrived at processing rx");
        assert!(result_tx.send(()).is_ok());
        event
    });

    let result = handle.dispatch(operation.clone(), topic).await;
    assert_eq!(result.processing_count, 1);
    assert_eq!(result.delivery_count, 0);
    assert!(result.await.is_ok());

    let event = event.await.unwrap();
    assert_eq!(event.operation, operation);
}

#[tokio::test]
async fn delivery_fan_out() {
    let topic_1 = Topic::random();
    let topic_2 = Topic::random();
    let topic_3 = Topic::random();

    let egress = Egress::new();

    let rx_1 = egress
        .register_delivery(topic_1, StreamType::Topic)
        .unwrap();

    let mut rx_2 = egress
        .register_delivery(topic_2, StreamType::Space)
        .unwrap();

    let mut rx_3 = egress
        .register_delivery(topic_3, StreamType::Space)
        .unwrap();

    let handle = egress.handle();

    let operation = {
        let signing_key = SigningKey::generate();
        let body = Body::from_bytes(b"plz deliver");
        let header = Header::builder()
            .body(&body)
            .build(&signing_key, Extensions::from_topic(topic_1));
        Operation::from_parts(header, Some(body))
    };

    let config = DispatchConfig {
        delivery: EventDeliveryPolicy::OnlySpaces,
        processing: EventProcessingPolicy::Disabled,
    };

    let result = handle
        .dispatch_with_config(operation.clone(), &config)
        .await;
    assert_eq!(result.processing_count, 0);
    assert_eq!(result.delivery_count, 2);

    // Nothing was sent on "topic" stream.
    assert_eq!(rx_1.size_hint(), (0, None));

    // Both "space" deliveries received one operation each.
    let (operation_2, _) = rx_2.next().await.expect("event arrived at processing rx");
    let (operation_3, _) = rx_3.next().await.expect("event arrived at processing rx");
    assert_eq!(operation_2, operation);
    assert_eq!(operation_3, operation);
}
