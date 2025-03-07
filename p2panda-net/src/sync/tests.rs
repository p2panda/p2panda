// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use futures_util::FutureExt;
use p2panda_core::PrivateKey;
use p2panda_sync::SyncError;
use p2panda_sync::test_protocols::{FailingProtocol, SyncTestTopic};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::engine::ToEngineActor;
use crate::sync;

/// Helper method to establish a sync session between the initiator and acceptor.
async fn run_sync_impl(
    protocol: FailingProtocol,
) -> (
    mpsc::Receiver<ToEngineActor<SyncTestTopic>>,
    mpsc::Receiver<ToEngineActor<SyncTestTopic>>,
    JoinHandle<Result<(), SyncError>>,
    JoinHandle<Result<(), SyncError>>,
) {
    let topic = SyncTestTopic::new("run test protocol impl");

    let initiator_node_id = PrivateKey::new().public_key();
    let acceptor_node_id = PrivateKey::new().public_key();

    let sync_protocol = Arc::new(protocol);

    // Duplex streams which simulate both ends of a bi-directional network connection.
    let (initiator_stream, acceptor_stream) = tokio::io::duplex(64 * 1024);
    let (initiator_read, initiator_write) = tokio::io::split(initiator_stream);
    let (acceptor_read, acceptor_write) = tokio::io::split(acceptor_stream);

    // Channel for sending messages out of a running sync session.
    let (initiator_tx, initiator_rx) = mpsc::channel(128);
    let (acceptor_tx, acceptor_rx) = mpsc::channel(128);

    let sync_protocol_clone = sync_protocol.clone();

    let initiator_handle = {
        let topic = topic.clone();

        tokio::spawn(async move {
            sync::initiate_sync(
                &mut initiator_write.compat_write(),
                &mut initiator_read.compat(),
                acceptor_node_id,
                topic.clone(),
                sync_protocol,
                initiator_tx,
            )
            .await
        })
    };

    let acceptor_handle = {
        tokio::spawn(async move {
            sync::accept_sync(
                &mut acceptor_write.compat_write(),
                &mut acceptor_read.compat(),
                initiator_node_id,
                sync_protocol_clone,
                acceptor_tx,
            )
            .await
        })
    };

    (initiator_rx, acceptor_rx, initiator_handle, acceptor_handle)
}

#[tokio::test]
async fn initiator_fails_critical() {
    let (mut rx_initiator, mut rx_acceptor, initiator_handle, acceptor_handle) =
        run_sync_impl(FailingProtocol::InitiatorFailsCritical).await;

    // Expected initiator messages.
    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    // Note: "SyncFailed" message is handled by manager for initiators.
    assert!(rx_initiator.recv().now_or_never().unwrap().is_none());

    // Expected acceptor messages.
    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncFailed { .. })
    ));

    // Expected handler results.
    assert_eq!(
        initiator_handle.await.unwrap(),
        Err(SyncError::Critical(
            "something really bad happened in the initiator".into(),
        ))
    );
    assert_eq!(
        acceptor_handle.await.unwrap(),
        // The acceptor failed as well, but only with an "unexpected behaviour" error over the
        // unexpectedly closed pipe.
        Err(SyncError::UnexpectedBehaviour("broken pipe".into()))
    );
}

#[tokio::test]
async fn initiator_fails_unexpected() {
    let (mut rx_initiator, mut rx_acceptor, initiator_handle, acceptor_handle) =
        run_sync_impl(FailingProtocol::InitiatorFailsUnexpected).await;

    // Expected initiator messages.
    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    // Note: "SyncFailed" message is handled by manager for initiators.
    assert!(rx_initiator.recv().now_or_never().unwrap().is_none());

    // Expected acceptor messages.
    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncFailed { .. })
    ));

    // Expected handler results.
    assert_eq!(
        initiator_handle.await.unwrap(),
        Err(SyncError::UnexpectedBehaviour("bang!".into(),))
    );
    assert_eq!(
        acceptor_handle.await.unwrap(),
        Err(SyncError::UnexpectedBehaviour("broken pipe".into()))
    );
}

#[tokio::test]
async fn initiator_sends_topic_twice() {
    let (mut rx_initiator, mut rx_acceptor, initiator_handle, acceptor_handle) =
        run_sync_impl(FailingProtocol::InitiatorSendsTopicTwice).await;

    // Expected initiator messages.
    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncDone { .. })
    ));

    // Expected acceptor messages.
    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncFailed { .. })
    ));

    assert_eq!(initiator_handle.await.unwrap(), Ok(()));
    assert_eq!(
        acceptor_handle.await.unwrap(),
        // This is _not_ a critical error as the acceptor protocol implementation handled the
        // protocol violation (sending topic twice) by itself.
        Err(SyncError::UnexpectedBehaviour(
            "received topic too often".into(),
        ))
    );
}

#[tokio::test]
async fn acceptor_fails_critical() {
    let (mut rx_initiator, mut rx_acceptor, initiator_handle, acceptor_handle) =
        run_sync_impl(FailingProtocol::AcceptorFailsCritical).await;

    // Expected initiator messages.
    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        // Initiator can end the session without any issues even when the acceptor fails.
        // @TODO: Do we want to detect the closed connection from the remote peer?
        Some(ToEngineActor::SyncDone { .. })
    ));

    // Expected acceptor messages.
    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncFailed { .. })
    ));

    // Expected handler results.
    assert_eq!(initiator_handle.await.unwrap(), Ok(()));
    assert_eq!(
        acceptor_handle.await.unwrap(),
        Err(SyncError::Critical(
            "something really bad happened in the acceptor".into(),
        ))
    );
}

#[tokio::test]
async fn acceptor_sends_topic() {
    let (mut rx_initiator, mut rx_acceptor, initiator_handle, acceptor_handle) =
        run_sync_impl(FailingProtocol::AcceptorSendsTopic).await;

    // Expected initiator messages.
    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    // Note: "SyncFailed" message is handled by manager for initiators.
    assert!(rx_initiator.recv().now_or_never().unwrap().is_none());

    // Expected acceptor messages.
    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncDone { .. })
    ));

    // Expected handler results.
    assert_eq!(
        initiator_handle.await.unwrap(),
        Err(SyncError::UnexpectedBehaviour(
            "unexpected message received from acceptor".into(),
        ))
    );
    assert_eq!(acceptor_handle.await.unwrap(), Ok(()));
}

#[tokio::test]
async fn run_sync_without_error() {
    let (mut rx_initiator, mut rx_acceptor, initiator_handle, acceptor_handle) =
        run_sync_impl(FailingProtocol::NoError).await;

    // Expected initiator messages.
    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_initiator.recv().await,
        Some(ToEngineActor::SyncDone { .. })
    ));

    // Expected acceptor messages.
    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncStart { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncHandshakeSuccess { .. })
    ));

    assert!(matches!(
        rx_acceptor.recv().await,
        Some(ToEngineActor::SyncDone { .. })
    ));

    // Expected handler results.
    assert_eq!(initiator_handle.await.unwrap(), Ok(()));
    assert_eq!(acceptor_handle.await.unwrap(), Ok(()));
}
