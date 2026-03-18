// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use futures_util::StreamExt;
use mock_instant::thread_local::MockClock;
use p2panda::operation::{LogId, Operation};
use p2panda::streams::{EphemeralMessage, Offset, ProcessedOperation, StreamEvent, SystemEvent};
use p2panda::test_utils::setup_logging;
use p2panda_core::{PrivateKey, Topic};
use p2panda_net::discovery::DiscoveryEvent;
use p2panda_store::logs::LogStore;
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
async fn ephemeral_stream() {
    let chat_id = Topic::new();

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

    let chat_id = Topic::new();

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
async fn replay_stream() {
    setup_logging();

    let chat_id = Topic::new();

    let panda = p2panda::builder().spawn().await.unwrap();
    let icebear = p2panda::builder().spawn().await.unwrap();

    // Panda subscribes to chat and publishes one message.
    {
        let (panda_tx, _panda_rx) = panda.stream::<String>(chat_id).await.unwrap();
        panda_tx.publish("Hello, Icebear!".into()).await.unwrap();
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Panda subscribes again, this time asking to replay all messages.
    let (_panda_tx, mut panda_rx) = panda
        .stream_from::<String>(chat_id, Offset::Start)
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
async fn event_stream() {
    setup_logging();

    let chat_id = Topic::new();

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

    let topic = Topic::new();

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
