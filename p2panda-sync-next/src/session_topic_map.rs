use std::collections::{HashMap, HashSet};

use futures::channel::mpsc;

use crate::{topic_log_sync::LiveModeMessage, traits::TopicQuery};

#[derive(Debug)]
pub struct SessionTopicMap<T, E> {
    pub(crate) accepting_sessions: HashSet<u64>,
    pub(crate) session_tx_map: HashMap<u64, mpsc::Sender<LiveModeMessage<E>>>,
    pub(crate) session_topic_map: HashMap<u64, T>,
    pub(crate) topic_session_map: HashMap<T, HashSet<u64>>,
}

impl<T, E> Default for SessionTopicMap<T, E> {
    fn default() -> Self {
        Self {
            accepting_sessions: Default::default(),
            session_tx_map: Default::default(),
            session_topic_map: Default::default(),
            topic_session_map: Default::default(),
        }
    }
}

impl<T, E> SessionTopicMap<T, E>
where
    T: TopicQuery,
{
    pub fn insert_with_topic(
        &mut self,
        session_id: u64,
        topic: T,
        tx: mpsc::Sender<LiveModeMessage<E>>,
    ) {
        self.session_topic_map.insert(session_id, topic.clone());
        self.topic_session_map
            .entry(topic.clone())
            .and_modify(|sessions| {
                sessions.insert(session_id);
            })
            .or_insert(HashSet::from_iter([session_id]));
        self.session_tx_map.insert(session_id, tx);
    }

    pub fn insert_accepting(&mut self, session_id: u64, tx: mpsc::Sender<LiveModeMessage<E>>) {
        self.accepting_sessions.insert(session_id);
        self.session_tx_map.insert(session_id, tx);
    }

    pub fn accepted(&mut self, session_id: u64, topic: T) -> bool {
        if self.accepting_sessions.remove(&session_id) {
            self.session_topic_map.insert(session_id, topic.clone());
            self.topic_session_map
                .entry(topic.clone())
                .and_modify(|sessions| {
                    sessions.insert(session_id);
                })
                .or_insert(HashSet::from_iter([session_id]));
        };

        true
    }

    pub fn drop(&mut self, session_id: u64) -> bool {
        if self.accepting_sessions.remove(&session_id) {
            self.session_tx_map.remove(&session_id);
            return true;
        };
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

    pub fn topic(&self, session_id: u64) -> Option<&T> {
        self.session_topic_map.get(&session_id)
    }

    pub fn sessions(&self, topic: &T) -> HashSet<u64> {
        self.topic_session_map
            .get(topic)
            .cloned()
            .unwrap_or_default()
    }

    pub fn session_channel(&self, session_id: u64) -> Option<mpsc::Sender<LiveModeMessage<E>>> {
        self.session_tx_map.get(&session_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use futures::channel::mpsc;

    use crate::{test_utils::TestTopic};
    use crate::session_topic_map::SessionTopicMap;

    const SESSION1: u64 = 1;
    const SESSION2: u64 = 2;
    const SESSION3: u64 = 3;

    const TOPIC_A: &str = "cats";
    const TOPIC_B: &str = "dogs";

    #[test]
    fn default_is_empty() {
        let map: SessionTopicMap<TestTopic, ()> = SessionTopicMap::default();
        assert!(map.accepting_sessions.is_empty());
        assert!(map.session_tx_map.is_empty());
        assert!(map.session_topic_map.is_empty());
        assert!(map.topic_session_map.is_empty());
    }

    #[test]
    fn insert_with_topic() {
        let (tx, _rx) = mpsc::channel(128);
        let mut map = SessionTopicMap::<_, ()>::default();

        map.insert_with_topic(SESSION1, TestTopic::new(TOPIC_A), tx.clone());

        // Check session→topic mapping
        assert_eq!(map.topic(SESSION1), Some(&TestTopic::new(TOPIC_A)));

        // Check topic→session mapping
        assert_eq!(
            map.sessions(&TestTopic::new(TOPIC_A)),
            HashSet::from_iter([SESSION1])
        );

        // Channel should be retrievable
        assert!(map.session_channel(SESSION1).is_some());
    }

    #[test]
    fn accept() {
        let (tx, _rx) = mpsc::channel(128);
        let mut map = SessionTopicMap::<_, ()>::default();

        map.insert_accepting(SESSION1, tx.clone());
        assert!(map.accepting_sessions.contains(&SESSION1));
        assert!(map.session_tx_map.contains_key(&SESSION1));

        map.accepted(SESSION1, TestTopic::new(TOPIC_A));

        assert!(!map.accepting_sessions.contains(&SESSION1));
        assert!(map.session_tx_map.contains_key(&SESSION1));
        assert_eq!(map.topic(SESSION1), Some(&TestTopic::new(TOPIC_A)));
        assert_eq!(
            map.sessions(&TestTopic::new(TOPIC_A)),
            HashSet::from([SESSION1])
        );
    }

    #[test]
    fn drop_accepting() {
        let (tx, _rx) = mpsc::channel(128);
        let mut map = SessionTopicMap::<TestTopic, ()>::default();

        map.insert_accepting(SESSION1, tx);
        assert!(map.drop(SESSION1));
        assert!(map.accepting_sessions.is_empty());
        assert!(map.session_tx_map.is_empty());
    }

    #[test]
    fn drop_session() {
        let (tx, _rx) = mpsc::channel(128);
        let mut map = SessionTopicMap::<TestTopic, ()>::default();

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
        let (tx1, _rx1) = mpsc::channel(128);
        let (tx2, _rx2) = mpsc::channel(128);
        let mut map = SessionTopicMap::<TestTopic, ()>::default();

        map.insert_with_topic(SESSION1, TestTopic::new(TOPIC_A), tx1);
        map.insert_with_topic(SESSION2, TestTopic::new(TOPIC_A), tx2);

        let sessions = map.sessions(&TestTopic::new(TOPIC_A));
        assert_eq!(sessions, HashSet::from([SESSION1, SESSION2]));
    }
}
