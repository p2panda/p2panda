// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Mapping of generic topics to session ids and of session ids to a channel sender.
#[derive(Clone, Debug)]
pub struct SessionTopicMap<T, TX> {
    pub(crate) session_tx_map: HashMap<u64, TX>,
    pub(crate) session_topic_map: HashMap<u64, T>,
    pub(crate) topic_session_map: HashMap<T, HashSet<u64>>,
}

impl<T, TX> Default for SessionTopicMap<T, TX> {
    fn default() -> Self {
        Self {
            session_tx_map: Default::default(),
            session_topic_map: Default::default(),
            topic_session_map: Default::default(),
        }
    }
}

impl<T, TX> SessionTopicMap<T, TX>
where
    T: Clone + Hash + Eq,
{
    /// Insert a session id with their topic and tx channel.
    pub fn insert_with_topic(&mut self, session_id: u64, topic: T, tx: TX) {
        self.session_topic_map.insert(session_id, topic.clone());
        self.topic_session_map
            .entry(topic.clone())
            .and_modify(|sessions| {
                sessions.insert(session_id);
            })
            .or_insert(HashSet::from_iter([session_id]));
        self.session_tx_map.insert(session_id, tx);
    }

    /// Drop a session from all mappings.
    ///
    /// Returns true if the session existed and was dropped, otherwise returns false when the
    /// session was known
    pub fn drop(&mut self, session_id: u64) -> bool {
        let Some(topic) = self.session_topic_map.remove(&session_id) else {
            return false;
        };
        self.topic_session_map
            .entry(topic.clone())
            .and_modify(|sessions| {
                sessions.remove(&session_id);
            });
        self.session_tx_map.remove(&session_id);
        true
    }

    /// Get the topic for a session id.
    ///
    /// Returns None of the session id was not known.
    pub fn topic(&self, session_id: u64) -> Option<&T> {
        self.session_topic_map.get(&session_id)
    }

    /// Get ids for all sessions associated with the given topic.
    pub fn sessions(&self, topic: &T) -> HashSet<u64> {
        self.topic_session_map
            .get(topic)
            .cloned()
            .unwrap_or_default()
    }

    /// Get a reference to a session sender.
    pub fn sender(&self, session_id: u64) -> Option<&TX> {
        self.session_tx_map.get(&session_id)
    }

    /// Get a mutable reference to a session sender.
    pub fn sender_mut(&mut self, session_id: u64) -> Option<&mut TX> {
        self.session_tx_map.get_mut(&session_id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use futures::channel::mpsc;

    use crate::SessionTopicMap;
    use crate::test_utils::TestTopic;

    const SESSION1: u64 = 1;
    const SESSION2: u64 = 2;
    const SESSION3: u64 = 3;

    const TOPIC_A: &str = "cats";
    const TOPIC_B: &str = "dogs";

    #[test]
    fn default_is_empty() {
        let map: SessionTopicMap<TestTopic, ()> = SessionTopicMap::default();
        assert!(map.session_tx_map.is_empty());
        assert!(map.session_topic_map.is_empty());
        assert!(map.topic_session_map.is_empty());
    }

    #[test]
    fn insert_with_topic() {
        let (tx, _rx) = mpsc::channel::<()>(128);
        let mut map = SessionTopicMap::default();

        map.insert_with_topic(SESSION1, TestTopic::new(TOPIC_A), tx.clone());

        // Check session→topic mapping
        assert_eq!(map.topic(SESSION1), Some(&TestTopic::new(TOPIC_A)));

        // Check topic→session mapping
        assert_eq!(
            map.sessions(&TestTopic::new(TOPIC_A)),
            HashSet::from_iter([SESSION1])
        );

        // Channel should be retrievable
        assert!(map.sender(SESSION1).is_some());
    }

    #[test]
    fn drop_session() {
        let (tx, _rx) = mpsc::channel::<()>(128);
        let mut map = SessionTopicMap::default();

        map.insert_with_topic(SESSION1, TestTopic::new(TOPIC_A), tx.clone());
        map.insert_with_topic(SESSION2, TestTopic::new(TOPIC_A), tx.clone());
        map.insert_with_topic(SESSION3, TestTopic::new(TOPIC_B), tx);

        // Drop one from topic A
        assert!(map.drop(SESSION1));

        // Should be removed from all mappings
        assert!(map.topic(SESSION1).is_none());
        assert!(!map.session_tx_map.contains_key(&SESSION1));

        // Remaining sessions for topic A
        let sessions = map.sessions(&TestTopic::new(TOPIC_A));
        assert_eq!(sessions, HashSet::from([SESSION2]));

        // Dropping a non-existent session returns false
        assert!(!map.drop(10));
    }

    #[test]
    fn insert_multiple_sessions_same_topic() {
        let (tx1, _rx1) = mpsc::channel::<()>(128);
        let (tx2, _rx2) = mpsc::channel::<()>(128);
        let mut map = SessionTopicMap::default();

        map.insert_with_topic(SESSION1, TestTopic::new(TOPIC_A), tx1);
        map.insert_with_topic(SESSION2, TestTopic::new(TOPIC_A), tx2);

        let sessions = map.sessions(&TestTopic::new(TOPIC_A));
        assert_eq!(sessions, HashSet::from([SESSION1, SESSION2]));
    }
}
