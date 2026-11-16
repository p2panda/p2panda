// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sync sessions in live mode and the payloads each has seen.
use std::collections::HashMap;

// TODO: switch to tokio channels as this is what we use in most other places. This requires
// changes in p2panda-sync where we expect the futures_channel versions.
use futures_channel::mpsc;
use futures_util::SinkExt;
use p2panda_core::Hash;
use p2panda_core::traits::Digest;
use p2panda_sync::ToSync;
use p2panda_sync::dedup::DeduplicationBuffer;
use tracing::debug;

/// Live-mode enabled sync sessions which are all interested in the same topic.
///
/// A live_tx sender and deduplication buffer is maintained for each session.
pub struct LiveSessions<OP>(HashMap<u64, LiveSession<OP>>);

/// The live_tx channel and deduplication buffer for a single session.
struct LiveSession<OP> {
    live_tx: mpsc::Sender<ToSync<OP>>,
    seen: DeduplicationBuffer<Hash>,
}

impl<OP> Default for LiveSessions<OP> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<OP> LiveSessions<OP>
where
    OP: Digest<Hash> + Clone,
{
    /// Register a new session.
    pub fn register(&mut self, session_id: u64, live_tx: mpsc::Sender<ToSync<OP>>) {
        self.0.insert(
            session_id,
            LiveSession {
                live_tx,
                seen: DeduplicationBuffer::default(),
            },
        );
    }

    /// A session has received a message.
    ///
    /// This marks the message as "seen" for the session it arrived on and forwards the message to
    /// any sessions which have not seen it.
    pub async fn received(&mut self, session_id: u64, operation: &OP) {
        let id = operation.hash();
        if let Some(session) = self.0.get_mut(&session_id) {
            session.seen.insert(id);
        }
        self.forward(id, operation).await;
    }

    /// Send a locally created message to all sessions.
    pub async fn send(&mut self, operation: &OP) {
        self.forward(operation.hash(), operation).await;
    }

    /// Send a close message to one session and remove it from the session map.
    pub async fn close(&mut self, session_id: u64) {
        if let Some(session) = self.0.remove(&session_id) {
            Self::close_session(session_id, session).await;
        }
    }

    /// Send a close message to all sessions and drain the session map.
    pub async fn close_all(&mut self) {
        for (session_id, session) in self.0.drain() {
            Self::close_session(session_id, session).await;
        }
    }

    async fn close_session(session_id: u64, session: LiveSession<OP>) {
        if session.live_tx.send(ToSync::Close).await.is_err() {
            debug!(session_id, "sync session already ended");
        }
    }

    /// Send an operation to all sessions which have not seen it.
    async fn forward(&mut self, id: Hash, operation: &OP) {
        let mut closed = vec![];
        for (session_id, session) in self.0.iter_mut() {
            if !session.seen.insert(id) {
                continue;
            }

            if let Err(err) = session
                .live_tx
                .send(ToSync::Payload(operation.clone()))
                .await
            {
                debug!(session_id, "sync session already ended: {err:?}");
                closed.push(*session_id);
            }
        }

        for id in closed {
            self.0.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::Hash;
    use p2panda_core::traits::Digest;
    use p2panda_sync::ToSync;
    use tokio::sync::mpsc;
    use tokio::sync::mpsc::error::TryRecvError;

    #[derive(Clone, Debug, PartialEq)]
    struct Operation(u64);

    impl Digest<Hash> for Operation {
        fn hash(&self) -> Hash {
            Hash::digest(self.0.to_le_bytes())
        }
    }

    impl Into<Operation> for u64 {
        fn into(self) -> Operation {
            Operation(self)
        }
    }

    type LiveSessions = super::LiveSessions<Operation>;

    fn register(sessions: &mut LiveSessions, session_id: u64) -> mpsc::Receiver<ToSync<Operation>> {
        let (tx, rx) = mpsc::channel::<ToSync<Operation>>(128);
        sessions.register(session_id, tx);
        rx
    }

    #[tokio::test]
    async fn register_session() {
        let mut sessions = LiveSessions::default();

        register(&mut sessions, 0);
        assert_eq!(sessions.0.len(), 1);
    }

    #[tokio::test]
    async fn send() {
        let mut sessions = LiveSessions::default();

        let mut rx0 = register(&mut sessions, 0);
        sessions.send(&0.into()).await;

        assert_eq!(rx0.recv().await, Some(ToSync::Payload(0.into())));

        let mut rx1 = register(&mut sessions, 1);
        sessions.send(&1.into()).await;

        // Two sessions are now registered, they should both receive the second message.
        assert_eq!(rx0.recv().await, Some(ToSync::Payload(1.into())));
        assert_eq!(rx1.recv().await, Some(ToSync::Payload(1.into())));
    }

    #[tokio::test]
    async fn receive_and_forward() {
        let mut sessions = LiveSessions::default();

        let mut rx0 = register(&mut sessions, 0);
        let mut rx1 = register(&mut sessions, 1);
        sessions.send(&0.into()).await;
        sessions.received(0, &0.into()).await;

        // Two sessions are registered, they should both receive the sent message.
        assert_eq!(rx0.recv().await, Some(ToSync::Payload(0.into())));
        assert_eq!(rx1.recv().await, Some(ToSync::Payload(0.into())));
    }

    #[tokio::test]
    async fn deduplicate_send_then_receive() {
        let mut sessions = LiveSessions::default();

        let op0 = 0.into();

        // Register session 0 and send an operation.
        let mut rx0 = register(&mut sessions, 0);
        sessions.send(&op0).await;
        assert_eq!(rx0.recv().await, Some(ToSync::Payload(0.into())));

        // Register session 1.
        let mut rx1 = register(&mut sessions, 1);

        // We receive the operation we just sent, it should only be forwarded to session 1 which
        // didn't see it yet.
        sessions.received(0, &0.into()).await;

        // Two sessions are registered, they should both receive the sent message.
        assert_eq!(rx0.try_recv().err(), Some(TryRecvError::Empty));
        assert_eq!(rx1.recv().await, Some(ToSync::Payload(0.into())));
    }

    #[tokio::test]
    async fn deduplicate_receive_then_receive() {
        let mut sessions = LiveSessions::default();

        let op0 = 0.into();

        // Register session 0 and receive an operation.
        let mut rx0 = register(&mut sessions, 0);
        sessions.received(0, &op0).await;

        // Nothing should be forwarded as the operation arrived on this session.
        assert_eq!(rx0.try_recv().err(), Some(TryRecvError::Empty));

        // Register session 1.
        let mut rx1 = register(&mut sessions, 1);

        // Receive another operation.
        sessions.received(0, &1.into()).await;

        // Again session 0 should not receive this, but session 1 should.
        assert_eq!(rx0.try_recv().err(), Some(TryRecvError::Empty));
        assert_eq!(rx1.recv().await, Some(ToSync::Payload(1.into())));
    }

    #[tokio::test]
    async fn close_and_remove() {
        let mut sessions = LiveSessions::default();

        let mut rx0 = register(&mut sessions, 0);
        let mut rx1 = register(&mut sessions, 1);
        let mut rx2 = register(&mut sessions, 2);

        sessions.close(0).await;
        assert_eq!(rx0.recv().await, Some(ToSync::Close));
        assert_eq!(rx0.try_recv().err(), Some(TryRecvError::Disconnected));
        assert!(sessions.0.get(&0).is_none());
        assert_eq!(rx1.try_recv().err(), Some(TryRecvError::Empty));
        assert_eq!(rx2.try_recv().err(), Some(TryRecvError::Empty));

        sessions.close_all().await;
        assert_eq!(rx1.recv().await, Some(ToSync::Close));
        assert_eq!(rx1.try_recv().err(), Some(TryRecvError::Disconnected));
        assert_eq!(rx2.recv().await, Some(ToSync::Close));
        assert_eq!(rx2.try_recv().err(), Some(TryRecvError::Disconnected));
    }
}
