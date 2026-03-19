// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{sync::Arc, time::Duration};

use iroh::Watcher;
use iroh::endpoint::{AfterHandshakeOutcome, ConnectionInfo, EndpointHooks};
use n0_future::task::AbortOnDropHandle;
use p2panda_core::PublicKey;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinSet;
use tracing::{Instrument, info, info_span};

use crate::iroh_endpoint::to_public_key;

/// Connection event.
#[derive(Clone, Debug)]
enum ConnectionEvent {
    Opened {
        remote_id: PublicKey,
        protocol: String,
        rtt: Option<Duration>,
    },
    Closed {
        remote_id: PublicKey,
        protocol: String,
        reason: String,
        udp_rx: Option<u64>,
        udp_tx: Option<u64>,
    },
}

/// Connection monitor.
#[derive(Clone, Debug)]
struct Monitor {
    tx: Sender<ConnectionInfo>,
    _task: Arc<AbortOnDropHandle<()>>,
}

impl EndpointHooks for Monitor {
    async fn after_handshake(&self, conn: &ConnectionInfo) -> AfterHandshakeOutcome {
        self.tx.send(conn.clone()).await.ok();
        AfterHandshakeOutcome::Accept
    }
}

impl Monitor {
    // TODO: We need a way to get the connection events out of the monitor.
    //
    // Pass in a sender and hold the receiver elsewhere.
    fn new() -> Self {
        let (tx, rx) = mpsc::channel(256);
        let task = tokio::spawn(Self::run(rx).instrument(info_span!("monitor")));
        Self {
            tx,
            _task: Arc::new(AbortOnDropHandle::new(task)),
        }
    }

    async fn run(mut rx: Receiver<ConnectionInfo>) {
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                Some(conn) = rx.recv() => {
                    let remote_id = to_public_key(conn.remote_id());
                    let protocol = String::from_utf8_lossy(conn.alpn()).to_string();
                    let rtt = conn.paths().peek().iter().map(|p| p.stats().rtt).min();
                    info!(%remote_id, %protocol, ?rtt, "new connection");
                    let opened_event = ConnectionEvent::Opened { remote_id, protocol: protocol.clone(), rtt };
                    tasks.spawn(async move {
                        // TODO: We can emit this info as events: connection closed.
                        match conn.closed().await {
                            Some((close_reason, stats)) => {
                                let udp_rx = stats.udp_rx.bytes;
                                let udp_tx = stats.udp_tx.bytes;
                                info!(%remote_id, %protocol, ?close_reason, udp_rx, udp_tx, "connection closed");
                                // TODO: For now we simply flatted the `close_reason` (iroh
                                // `ConnectionError`) into a `String`; this may be sufficient for
                                // consumers.
                                let ended_event = ConnectionEvent::Closed { remote_id, protocol, reason: close_reason.to_string(), udp_rx: Some(udp_rx), udp_tx: Some(udp_tx) };
                            }
                            None => {
                                // The connection was closed before we could register our stats-on-close listener.
                                let reason = "connection closed before tracking started";
                                info!(%remote_id, %protocol, reason);
                                let ended_event = ConnectionEvent::Closed { remote_id, protocol, reason: reason.to_string(), udp_rx: None, udp_tx: None };
                            }
                        }
                    }.instrument(tracing::Span::current()));
                }
                Some(res) = tasks.join_next(), if !tasks.is_empty() => res.expect("conn close task panicked"),
                else => break,
            }
            while let Some(res) = tasks.join_next().await {
                res.expect("conn close task panicked");
            }
        }
    }
}
