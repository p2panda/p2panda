// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use futures_util::StreamExt;
use mock_instant::thread_local::MockClock;
use p2panda::Credentials;
use p2panda::node::AckPolicy;
use p2panda::operation::{Extensions, LogId, Operation};
use p2panda::streams::{
    EphemeralMessage, ProcessedOperation, StreamEvent, StreamFrom, SystemEvent,
};
use p2panda_core::cbor::encode_cbor;
use p2panda_core::logs::LogHeights;
use p2panda_core::test_utils::{TestLog, setup_logging};
use p2panda_core::{Cursor, Hash, Topic};
use p2panda_net::discovery::DiscoveryEvent;
use p2panda_store::logs::LogStore;
use tokio::task::JoinHandle;

fn assert_replay_started<M>(event: &StreamEvent<M>, expected_total_operations: u32) {
    let StreamEvent::ReplayStarted { total_operations } = event else {
        panic!("unexpected event");
    };
    assert_eq!(total_operations, &expected_total_operations);
}

fn assert_replay_ended<M>(event: &StreamEvent<M>) {
    let StreamEvent::ReplayEnded = event else {
        panic!("unexpected event");
    };
}

fn assert_message_id<M>(event: &StreamEvent<M>, id: Hash) {
    let StreamEvent::Processed { operation, .. } = event else {
        panic!("unexpected event");
    };
    assert_eq!(operation.id(), id);
}

#[tokio::test]
async fn build_and_spawn() -> Result<(), Box<dyn std::error::Error>> {
    // Default & instant setup.
    let _node = p2panda::spawn().await?;

    // Customizable "builder" setup flow.
    let _node = p2panda::builder()
        .database_url("sqlite::memory:")
        .credentials(Credentials::generate())
        .spawn()
        .await?;

    Ok(())
}

#[tokio::test]
async fn ephemeral_stream() {
    let chat_id = Topic::random();

    let panda = p2panda::spawn().await.unwrap();
    let icebear = p2panda::spawn().await.unwrap();

    // Panda joins the chat and sends a message to icebear, then waits for an answer.
    let panda_task: JoinHandle<EphemeralMessage<String>> = {
        let (tx, mut rx) = panda.ephemeral_stream(chat_id).await.unwrap();

        tokio::spawn(async move {
            tx.publish("Hello, Icebear!".into()).await.unwrap();
            let message = rx.next().await.unwrap();
            message
        })
    };

    // Icebear joins the chat and waits for a message of panda, to then answer.
    let icebear_task: JoinHandle<EphemeralMessage<String>> = {
        let (tx, mut rx) = icebear.ephemeral_stream(chat_id).await.unwrap();

        tokio::spawn(async move {
            let message = rx.next().await.unwrap();
            // Advance the clock to ensure icebear's message is published later than panda's.
            MockClock::advance_system_time(Duration::from_secs(1));
            tx.publish("Hi, Panda!".into()).await.unwrap();
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

#[tokio::test]
async fn eventually_consistent_stream() {
    setup_logging();

    let chat_id = Topic::random();

    let panda = p2panda::builder().spawn().await.unwrap();
    let icebear = p2panda::builder().spawn().await.unwrap();

    // Panda joins the chat and sends a message to icebear.
    let (panda_tx, _panda_rx) = panda.stream::<String>(chat_id).await.unwrap();
    panda_tx.publish("Hello, Icebear!".into()).await.unwrap();

    // Icebear joins the chat and waits for a message of panda.
    let (_icebear_tx, mut icebear_rx) = icebear.stream::<String>(chat_id).await.unwrap();

    let mut received: Option<ProcessedOperation<String>> = None;

    while let Some(event) = icebear_rx.next().await {
        if let StreamEvent::Processed { operation, .. } = event {
            received = Some(operation);
            break;
        }
    }

    let received = received.expect("icebear should have received operation");
    assert_eq!(received.message(), &"Hello, Icebear!".to_string());
    assert_eq!(received.author(), panda.id());
}

#[tokio::test]
async fn event_stream() {
    setup_logging();

    let chat_id = Topic::random();

    let panda = p2panda::builder().spawn().await.unwrap();
    let icebear = p2panda::builder().spawn().await.unwrap();

    // Panda joins the chat and sends a message to icebear.
    let (panda_tx, _panda_rx) = panda.stream::<String>(chat_id).await.unwrap();
    panda_tx.publish("Hello, Icebear!".into()).await.unwrap();

    // Icebear joins the chat.
    let (_icebear_tx, _icebear_rx) = icebear.stream::<String>(chat_id).await.unwrap();

    // Create a system event stream for panda.
    let mut events = panda.event_stream().await.unwrap();

    let mut received_event = false;

    // Wait for the first discovery session started event.
    while let Some(event) = events.next().await {
        if let SystemEvent::Discovery(DiscoveryEvent::SessionStarted { .. }) = event {
            received_event = true;
            break;
        }
    }

    assert!(received_event);
}

#[tokio::test]
async fn log_prefix_pruning() {
    setup_logging();

    let topic = Topic::random();

    let panda = p2panda::builder().spawn().await.unwrap();
    let icebear = p2panda::builder().spawn().await.unwrap();

    let (panda_tx, _) = panda.stream::<usize>(topic).await.unwrap();

    // 1. Panda publishes 3 operations into their append-only log.
    panda_tx.publish(1).await.unwrap();
    panda_tx.publish(2).await.unwrap();
    panda_tx.publish(3).await.unwrap();

    // 2. Icebear joins the topic and starts syncing Panda's operations. Please note that due to
    //    async behaviour we don't know how many operations Icebear will _exactly_ sync before
    //    pruning takes place.
    let (_, mut icebear_rx) = icebear.stream::<usize>(topic).await.unwrap();

    // 3. Panda prunes their log now and sets the last message to be "4".
    let processing = panda_tx.prune(Some(4)).await.unwrap();

    // We keep around the hash of the operation which pruned the log.
    let hash = processing.hash();

    // 4. Panda waits until their pruning operation was successfully processed in their local
    //    processing pipeline.
    let result = processing.await.unwrap();
    assert!(result.is_completed());
    assert!(!result.is_failed());

    // 5. We wait until icebear processed the (from their perspective remotely incoming) pruning
    //    operation as well.
    while let Some(event) = icebear_rx.next().await {
        if let StreamEvent::Processed { operation, .. } = event {
            assert!(operation.processed().is_completed());
            assert!(!operation.processed().is_failed());

            if operation.id() == hash {
                break;
            }
        }
    }

    // There should only be 1 message in Panda's and Icebear's database as the log was pruned.
    let log_id = LogId::from_topic(topic);
    let panda_result: Vec<(Operation, Vec<u8>)> = panda
        .store()
        .get_log_entries(&panda.id(), &log_id, None, None)
        .await
        .expect("no store failure")
        .expect("result to be Some");
    assert_eq!(panda_result.iter().count(), 1);

    let icebear_result: Vec<(Operation, Vec<u8>)> = icebear
        .store()
        .get_log_entries(&panda.id(), &log_id, None, None)
        .await
        .expect("no store failure")
        .expect("result to be Some");
    assert_eq!(icebear_result.iter().count(), 1);
}

#[tokio::test]
async fn automatic_acking() {
    setup_logging();

    let topic = Topic::random();
    let node = p2panda::builder().spawn().await.unwrap();

    let (tx, mut rx) = node.stream::<String>(topic).await.unwrap();

    // Publish two messages into the stream.
    let processing = tx.publish("first".into()).await.unwrap();
    let message_id_1 = processing.hash();
    processing.await.unwrap();

    let processing = tx.publish("second".into()).await.unwrap();
    let message_id_2 = processing.hash();
    processing.await.unwrap();

    // We except to receive them.
    assert_message_id(&rx.next().await.unwrap(), message_id_1);
    assert_message_id(&rx.next().await.unwrap(), message_id_2);

    // Create a new subscription.
    drop(tx);
    drop(rx);

    let (tx, mut rx) = node.stream::<String>(topic).await.unwrap();

    // Publish one more message.
    let processing = tx.publish("third".into()).await.unwrap();
    let message_id_3 = processing.hash();
    processing.await.unwrap();

    // We expect to only receive the new, third message and not the previous two ones anymore.
    assert_message_id(&rx.next().await.unwrap(), message_id_3);
}

#[tokio::test]
async fn explicit_acking() {
    setup_logging();

    let topic = Topic::random();
    let node = p2panda::builder()
        .ack_policy(AckPolicy::Explicit)
        .spawn()
        .await
        .unwrap();

    let (tx, mut rx) = node.stream::<String>(topic).await.unwrap();

    // Publish two messages into the stream.
    let message_id_1 = {
        let processing = tx.publish("first".into()).await.unwrap();
        let id = processing.hash();
        processing.await.unwrap();
        id
    };

    let message_id_2 = {
        let processing = tx.publish("second".into()).await.unwrap();
        let id = processing.hash();
        processing.await.unwrap();
        id
    };

    // We except to receive them from the subscription stream.
    assert_message_id(&rx.next().await.unwrap(), message_id_1);
    assert_message_id(&rx.next().await.unwrap(), message_id_2);

    // Acknowledge first message.
    rx.ack(message_id_1).await.unwrap();

    // Create a new subscription, streaming from "acked frontier".
    drop(tx);
    drop(rx);

    let (_tx, mut rx) = node.stream::<String>(topic).await.unwrap();

    // We except to receive only the un-acked messages from the subscription stream.
    assert_replay_started(&rx.next().await.unwrap(), 1);
    assert_message_id(&rx.next().await.unwrap(), message_id_2);
    assert_replay_ended(&rx.next().await.unwrap());
}

#[tokio::test]
async fn replay_stream_from_start() {
    setup_logging();

    let chat_id = Topic::random();

    let panda = p2panda::builder().spawn().await.unwrap();
    let icebear = p2panda::builder().spawn().await.unwrap();

    // Panda subscribes to chat and publishes one message.
    {
        let (panda_tx, _panda_rx) = panda.stream::<String>(chat_id).await.unwrap();
        panda_tx.publish("Hello, Icebear!".into()).await.unwrap();
    }

    // Panda subscribes again, this time asking to replay all messages from start.
    let (_panda_tx, mut panda_rx) = panda
        .stream_from::<String>(chat_id, StreamFrom::Start)
        .await
        .unwrap();

    // Icebear joins the chat and publishes one message.
    let (icebear_tx, _icebear_rx) = icebear.stream::<String>(chat_id).await.unwrap();
    icebear_tx.publish("Hello, Panda!".into()).await.unwrap();

    // Panda should receive the first message they sent again, followed by Icebear's message which
    // arrived via sync.
    let mut received = vec![];

    while let Some(event) = panda_rx.next().await {
        if let StreamEvent::Processed { operation, .. } = event {
            received.push(operation);
            if received.len() == 2 {
                break;
            }
        }
    }

    assert_eq!(received[0].message(), &"Hello, Icebear!".to_string());
    assert_eq!(received[1].message(), &"Hello, Panda!".to_string());
}

#[tokio::test]
async fn replay_stream_from_cursor() {
    setup_logging();

    let topic = Topic::random();
    let node = p2panda::builder().spawn().await.unwrap();

    let (tx, rx) = node.stream::<String>(topic).await.unwrap();

    // Publish two messages into the stream.
    let _message_id_1 = {
        let processing = tx.publish("first".into()).await.unwrap();
        let id = processing.hash();
        processing.await.unwrap();
        id
    };

    let message_id_2 = {
        let processing = tx.publish("second".into()).await.unwrap();
        let id = processing.hash();
        processing.await.unwrap();
        id
    };

    let message_id_3 = {
        let processing = tx.publish("third".into()).await.unwrap();
        let id = processing.hash();
        processing.await.unwrap();
        id
    };

    // Force re-playing from custom cursor position with new stream subscription.
    drop(tx);
    drop(rx);

    let mut cursor = Cursor::new(topic.to_string(), LogHeights::default());
    cursor.advance(node.id(), LogId::from_topic(topic), 0); // seq_num = 0, the first message

    let (_tx, mut rx) = node
        .stream_from::<String>(topic, StreamFrom::Cursor(cursor))
        .await
        .unwrap();

    // We expect to only receive the second and third message.
    assert_replay_started(&rx.next().await.unwrap(), 2);
    assert_message_id(&rx.next().await.unwrap(), message_id_2);
    assert_message_id(&rx.next().await.unwrap(), message_id_3);
    assert_replay_ended(&rx.next().await.unwrap());
}

#[tokio::test]
async fn import_external_stream() {
    setup_logging();

    let chat_id = Topic::random();

    // Panda opens their app and publishes some messages into a chat.
    let panda_log = TestLog::new();
    let extensions = Extensions::from_topic(chat_id);

    let operation_1 = panda_log.operation(
        &encode_cbor(&"Hello, Icebear!").unwrap(),
        extensions.clone(),
    );
    let operation_2 = panda_log.operation(
        &encode_cbor(&"I'm in a remote place with no internet, it's really nice :-p").unwrap(),
        extensions.clone(),
    );
    let operation_3 = panda_log.operation(
        &encode_cbor(&"Gunna post these messages to you on an SD card yo!").unwrap(),
        extensions,
    );

    // Panda exports messages to an SD card.
    let exported = vec![
        operation_1.clone(),
        operation_2.clone(),
        operation_3.clone(),
    ];

    // Panda goes offline and walks to the post office to send the SD card to Icebear.

    // Icebear receives the SD card, opens their app and initiates import.
    let import_stream = futures_util::stream::iter(exported);
    let icebear = p2panda::builder().spawn().await.unwrap();
    let (icebear_tx, mut icebear_rx) = icebear.stream::<String>(chat_id).await.unwrap();
    let import = icebear_tx.import(import_stream).await.unwrap();

    assert_eq!(import.session_id(), 0);
    assert!(import.await.is_ok());

    // Icebear receives the new messages after they've been processed.
    let mut imported = Vec::new();
    let mut start_received = false;
    let mut end_received = false;
    while let Some(event) = icebear_rx.next().await {
        if let StreamEvent::ImportStarted { session_id } = &event {
            assert!(!start_received);
            assert_eq!(session_id, &0);
            start_received = true;
            continue;
        };

        if let StreamEvent::Processed { operation, .. } = &event {
            assert!(start_received);
            assert!(!end_received);
            imported.push(operation.clone());
            if imported.len() == 3 {
                continue;
            }
        }

        if let StreamEvent::ImportEnded { session_id } = event {
            assert!(start_received);
            assert!(!end_received);
            assert_eq!(session_id, 0);
            end_received = true;
            break;
        };
    }

    assert!(start_received);
    assert!(end_received);
    assert_eq!(imported.len(), 3);
    assert!(
        imported
            .iter()
            .any(|event| event.id() == operation_1.header().hash())
    );
    assert!(
        imported
            .iter()
            .any(|event| event.id() == operation_2.header().hash())
    );
    assert!(
        imported
            .iter()
            .any(|event| event.id() == operation_3.header().hash())
    );
    assert!(end_received);
}

#[tokio::test]
async fn deduplicate_events() {
    setup_logging();

    let chat_id = Topic::random();

    let panda = p2panda::builder().spawn().await.unwrap();
    let icebear = p2panda::builder().spawn().await.unwrap();

    // Panda joins the chat.
    let (panda_tx, _panda_rx) = panda.stream::<String>(chat_id).await.unwrap();

    // Icebear joins the chat and waits for a message of panda.
    let (_icebear_tx, mut icebear_rx) = icebear.stream::<String>(chat_id).await.unwrap();

    panda_tx.publish("Hello, Icebear!".into()).await.unwrap();
    panda_tx
        .publish("Hello, Icebear again!".into())
        .await
        .unwrap();
    panda_tx
        .publish("Hello, Icebear and again!".into())
        .await
        .unwrap();

    let mut received = vec![];

    loop {
        let event = icebear_rx.next().await.unwrap();
        if let StreamEvent::SyncStarted { .. } = event {
            break;
        }
    }

    loop {
        tokio::select! {
            Some(event) = icebear_rx.next() => {
                if let StreamEvent::Processed { operation, .. } = event {
                    received.push(operation);
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                break;
            }
        }
    }

    assert_eq!(received.len(), 3);
}

#[tokio::test]
async fn stop_sync_on_drop() {
    setup_logging();

    let chat_id = Topic::random();

    let panda = p2panda::builder().spawn().await.unwrap();
    let icebear = p2panda::builder().spawn().await.unwrap();

    // Panda joins the chat and sends a message to icebear.
    let (panda_tx, panda_rx) = panda.stream::<String>(chat_id).await.unwrap();
    panda_tx.publish("Hello, Icebear!".into()).await.unwrap();

    // Icebear joins the chat.
    let (_icebear_tx, mut icebear_rx) = icebear.stream::<String>(chat_id).await.unwrap();

    let mut sync_started = false;
    let mut message_received = false;
    let mut sync_ended = false;

    // Wait for icebear to begin a sync session with panda.
    if let Some(event) = icebear_rx.next().await {
        if let StreamEvent::SyncStarted { .. } = event {
            sync_started = true;
        }
    }
    // Wait for icebear to receive panda's message.
    if let Some(event) = icebear_rx.next().await {
        if let StreamEvent::Processed { .. } = event {
            message_received = true;
        }
    }

    drop(panda_tx);
    drop(panda_rx);

    // Ensure that the sync session ends when panda's publisher and subscription are dropped.
    if let Some(event) = icebear_rx.next().await {
        if let StreamEvent::SyncEnded { .. } = event {
            sync_ended = true;
        }
    }

    assert!(sync_started);
    assert!(message_received);
    assert!(sync_ended);
}
