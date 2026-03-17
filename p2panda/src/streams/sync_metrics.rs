// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};

use p2panda_core::{Extensions, Operation};
use p2panda_net::NodeId;
use p2panda_sync::FromSync;
use p2panda_sync::protocols::{Metrics, TopicLogSyncEvent};
use thiserror::Error;

use crate::streams::StreamEvent;
use crate::streams::stream::Source;

type SessionId = u64;

/// Track state of all running sync sessions for a topic and calculate useful aggregate data.
#[derive(Clone, Debug, Default)]
pub struct Aggregator {
    /// Total number of running sync sessions for a topic.
    running_sessions: u64,

    /// Total number of bytes sent across all topic sessions.
    total_bytes_sent: u64,

    /// Total number of received bytes across all topic sessions.
    total_bytes_received: u64,

    /// Latest metrics for all sessions.
    session_metrics: HashMap<SessionId, Metrics>,

    /// Set of running sessions which have entered live mode.
    live_mode: HashSet<SessionId>,
}

impl Aggregator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a `TopicLogSyncEvent`, collect metrics and calculate aggregates and return
    /// enriched aggregate event types.
    pub fn process<E: Extensions>(
        &mut self,
        from_sync: FromSync<TopicLogSyncEvent<E>>,
    ) -> Option<SyncEvent<E>> {
        let FromSync {
            session_id,
            remote,
            event,
            ..
        } = from_sync;

        match event {
            TopicLogSyncEvent::SessionStarted => {
                self.running_sessions += 1;
                // Insert default metrics into the session metrics map for now, these will be
                // removed or over-written on the next event.
                self.session_metrics.insert(session_id, Metrics::default());
                None
            }
            TopicLogSyncEvent::SyncStarted { metrics } => {
                self.session_metrics.insert(session_id, metrics.clone());
                Some(SyncEvent::SyncStarted {
                    remote,
                    session_id,
                    incoming_operations: metrics.inbound_sync_operations,
                    outgoing_operations: metrics.outbound_sync_operations,
                    incoming_bytes: metrics.inbound_sync_bytes,
                    outgoing_bytes: metrics.outbound_sync_bytes,
                    topic_sessions: self.running_sessions(),
                })
            }
            TopicLogSyncEvent::OperationReceived { operation, metrics } => {
                self.session_metrics.insert(session_id, metrics.clone());
                Some(SyncEvent::OperationReceived {
                    operation,
                    source: Source::SyncSession {
                        remote_node_id: remote,
                        session_id,
                        sent_bytes: metrics.sent_bytes(),
                        received_bytes: metrics.received_bytes(),
                        sent_operations: metrics.sent_operations(),
                        received_operations: metrics.received_operations(),
                        sent_bytes_topic_total: self.total_bytes_sent(),
                        received_bytes_topic_total: self.total_bytes_received(),
                        phase: if self.live_mode.contains(&session_id) {
                            SessionPhase::Live
                        } else {
                            SessionPhase::Sync
                        },
                    },
                })
            }
            TopicLogSyncEvent::SyncFinished { metrics } => {
                self.session_metrics.insert(session_id, metrics.clone());
                self.total_bytes_sent += metrics.sent_bytes();
                self.total_bytes_received += metrics.received_bytes();
                None
            }
            TopicLogSyncEvent::SessionFinished { metrics } => {
                self.handle_session_end(session_id);
                self.total_bytes_sent += metrics.sent_bytes();
                self.total_bytes_received += metrics.received_bytes();
                Some(SyncEvent::SyncEnded {
                    remote,
                    session_id,
                    sent_bytes: metrics.sent_bytes(),
                    received_bytes: metrics.received_bytes(),
                    sent_operations: metrics.sent_operations(),
                    received_operations: metrics.received_operations(),
                    sent_bytes_topic_total: self.total_bytes_sent(),
                    received_bytes_topic_total: self.total_bytes_received(),
                    error: None,
                })
            }
            TopicLogSyncEvent::Failed { error } => {
                let metrics = self.handle_session_end(session_id);
                Some(SyncEvent::SyncEnded {
                    remote,
                    session_id,
                    sent_bytes: metrics.sent_bytes(),
                    received_bytes: metrics.received_bytes(),
                    sent_operations: metrics.sent_operations(),
                    received_operations: metrics.received_operations(),
                    sent_bytes_topic_total: self.total_bytes_sent(),
                    received_bytes_topic_total: self.total_bytes_received(),
                    error: Some(SyncError(error)),
                })
            }
            TopicLogSyncEvent::LiveModeStarted => {
                self.live_mode.insert(session_id);
                None
            }
        }
    }

    fn handle_session_end(&mut self, session_id: SessionId) -> Metrics {
        self.running_sessions = self.running_sessions.saturating_sub(1);
        self.live_mode.remove(&session_id);
        self.session_metrics.remove(&session_id).unwrap_or_default()
    }

    /// Total running sessions for a topic.
    pub fn running_sessions(&self) -> u64 {
        self.running_sessions
    }

    /// Total bytes sent on a topic.
    pub fn total_bytes_sent(&self) -> u64 {
        self.total_bytes_sent
    }

    /// Total bytes received on a topic.
    pub fn total_bytes_received(&self) -> u64 {
        self.total_bytes_received
    }
}

/// Which phase of a sync session an operation arrived in.
#[derive(Clone, Debug)]
pub enum SessionPhase {
    Sync,
    Live,
}

/// Intermediate sync event type enriched with aggregate data.
#[derive(Debug)]
pub(crate) enum SyncEvent<E> {
    SyncStarted {
        remote: NodeId,
        session_id: u64,
        incoming_operations: u64,
        outgoing_operations: u64,
        incoming_bytes: u64,
        outgoing_bytes: u64,
        topic_sessions: u64,
    },
    SyncEnded {
        remote: NodeId,
        session_id: u64,
        sent_operations: u64,
        received_operations: u64,
        sent_bytes: u64,
        received_bytes: u64,
        sent_bytes_topic_total: u64,
        received_bytes_topic_total: u64,
        error: Option<SyncError>,
    },
    OperationReceived {
        operation: Box<Operation<E>>,
        source: Source,
    },
}

impl<E, M> From<SyncEvent<E>> for StreamEvent<M> {
    fn from(value: SyncEvent<E>) -> Self {
        match value {
            SyncEvent::SyncStarted {
                remote,
                session_id,
                incoming_operations,
                outgoing_operations,
                incoming_bytes,
                outgoing_bytes,
                topic_sessions,
            } => StreamEvent::SyncStarted {
                remote_node_id: remote,
                session_id,
                incoming_operations,
                outgoing_operations,
                incoming_bytes,
                outgoing_bytes,
                topic_sessions,
            },
            SyncEvent::SyncEnded {
                remote,
                session_id,
                sent_operations,
                received_operations,
                sent_bytes,
                received_bytes,
                sent_bytes_topic_total,
                received_bytes_topic_total,
                error,
            } => StreamEvent::SyncEnded {
                remote_node_id: remote,
                session_id,
                sent_operations,
                received_operations,
                sent_bytes,
                received_bytes,
                sent_bytes_topic_total,
                received_bytes_topic_total,
                error,
            },
            // We can't convert operation events simply like this as they need to be processed and
            // decoded first so this branch is never called.
            SyncEvent::OperationReceived { .. } => unreachable!(),
        }
    }
}

#[derive(Clone, Debug, Error)]
#[error("an error occurred during sync: {0}")]
pub struct SyncError(String);

#[cfg(test)]
mod tests {
    use p2panda_net::NodeId;
    use p2panda_sync::FromSync;
    use p2panda_sync::protocols::{Metrics, TopicLogSyncEvent};

    use crate::streams::sync_metrics::{Aggregator, SyncEvent};

    fn from_sync(session_id: u64, event: TopicLogSyncEvent<()>) -> FromSync<TopicLogSyncEvent<()>> {
        FromSync {
            session_id,
            remote: NodeId::default(),
            event,
        }
    }

    #[test]
    fn running_sessions() {
        let mut aggregator = Aggregator::new();

        aggregator.process(from_sync(1, TopicLogSyncEvent::SessionStarted));
        aggregator.process(from_sync(2, TopicLogSyncEvent::SessionStarted));
        assert_eq!(aggregator.running_sessions(), 2);

        aggregator.process(from_sync(
            1,
            TopicLogSyncEvent::SessionFinished {
                metrics: Metrics::default(),
            },
        ));
        assert_eq!(aggregator.running_sessions(), 1);
    }

    #[test]
    fn failed_session() {
        let mut aggregator = Aggregator::new();

        aggregator.process(from_sync(1, TopicLogSyncEvent::SessionStarted));
        assert_eq!(aggregator.running_sessions(), 1);

        let result = aggregator.process(from_sync(
            1,
            TopicLogSyncEvent::Failed {
                error: "connection dropped".to_string(),
            },
        ));
        assert_eq!(aggregator.running_sessions(), 0);

        match result {
            Some(SyncEvent::SyncEnded { error, .. }) => {
                assert!(error.is_some(), "expected an error on failed session");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn aggregate_bytes() {
        let mut aggregator = Aggregator::new();

        let metrics_start_a = Metrics {
            inbound_sync_bytes: 100,
            outbound_sync_bytes: 50,
            ..Default::default()
        };

        let metrics_start_b = Metrics {
            inbound_sync_bytes: 100,
            outbound_sync_bytes: 80,
            ..Default::default()
        };

        let metrics_end_a = Metrics {
            inbound_sync_bytes: 100,
            outbound_sync_bytes: 50,
            received_sync_bytes: 100,
            sent_sync_bytes: 50,
            ..Default::default()
        };

        let metrics_end_b = Metrics {
            inbound_sync_bytes: 100,
            outbound_sync_bytes: 50,
            received_sync_bytes: 100,
            sent_sync_bytes: 80,
            ..Default::default()
        };

        aggregator.process(from_sync(1, TopicLogSyncEvent::SessionStarted));
        aggregator.process(from_sync(
            1,
            TopicLogSyncEvent::SyncStarted {
                metrics: metrics_start_a,
            },
        ));
        aggregator.process(from_sync(
            1,
            TopicLogSyncEvent::SyncFinished {
                metrics: metrics_end_a,
            },
        ));
        aggregator.process(from_sync(2, TopicLogSyncEvent::SessionStarted));
        aggregator.process(from_sync(
            2,
            TopicLogSyncEvent::SyncStarted {
                metrics: metrics_start_b,
            },
        ));
        aggregator.process(from_sync(
            2,
            TopicLogSyncEvent::SyncFinished {
                metrics: metrics_end_b,
            },
        ));

        assert_eq!(aggregator.total_bytes_received(), 200);
        assert_eq!(aggregator.total_bytes_sent(), 130);
    }
}
