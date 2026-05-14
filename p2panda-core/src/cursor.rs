// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;

use crate::identity::Author;
use crate::logs::{LogHeights, LogId, LogRanges, SeqNum, compare};

/// Cursor to track log heights (state vector).
///
/// It offers methods to "advance" a log and compute the difference to another state vector. A
/// cursor can be used to manage a state vector over a topic ("log heights" of logs scoped by a
/// topic).
#[derive(Clone, Debug, Ord, PartialOrd, PartialEq, Eq, StdHash)]
pub struct Cursor<A, L> {
    name: String,
    state: LogHeights<A, L>,
}

impl<A, L> Cursor<A, L>
where
    A: Author,
    L: LogId,
{
    pub fn new(name: impl AsRef<str>, state: LogHeights<A, L>) -> Self {
        Self {
            name: name.as_ref().to_string(),
            state,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns state vector.
    pub fn state(&self) -> &LogHeights<A, L> {
        &self.state
    }

    /// Returns state vector for a specific log.
    pub fn log_height(&self, author: &A, log_id: &L) -> Option<&SeqNum> {
        self.state.get(author).and_then(|logs| logs.get(log_id))
    }

    /// Calculates the difference between two state vectors.
    pub fn compare(&self, other: &LogHeights<A, L>) -> LogRanges<A, L> {
        compare(other, &self.state)
    }

    /// Advances the state of a specific log.
    pub fn advance(&mut self, author: A, log_id: L, log_height: SeqNum) {
        // Ignore if given log-height is lower-or-equal than current state.
        if let Some(current_log_height) = self.log_height(&author, &log_id)
            && current_log_height >= &log_height
        {
            return;
        }

        self.state
            .entry(author)
            .or_default()
            .insert(log_id, log_height);
    }
}

#[cfg(test)]
mod tests {
    use crate::logs::LogHeights;
    use crate::{SigningKey, VerifyingKey};

    use super::Cursor;

    #[test]
    fn advance_log_height() {
        let author_1 = SigningKey::generate().verifying_key();
        let author_2 = SigningKey::generate().verifying_key();

        let mut cursor = Cursor::<VerifyingKey, u64>::new("test", LogHeights::default());
        assert_eq!(cursor.name(), "test");

        assert!(cursor.log_height(&author_1, &0).is_none());
        assert!(cursor.log_height(&author_2, &0).is_none());

        cursor.advance(author_1, 0, 23);
        assert_eq!(cursor.log_height(&author_1, &0), Some(&23));
        assert!(cursor.log_height(&author_2, &0).is_none());

        cursor.advance(author_2, 0, 10);
        cursor.advance(author_2, 1, 2);
        assert_eq!(cursor.log_height(&author_1, &0), Some(&23));
        assert_eq!(cursor.log_height(&author_2, &0), Some(&10));
        assert_eq!(cursor.log_height(&author_2, &1), Some(&2));
    }

    #[test]
    fn strict_monotonic_incremental() {
        let author = SigningKey::generate().verifying_key();
        let mut cursor = Cursor::<VerifyingKey, u64>::new("test", LogHeights::default());

        // Ignore attempts to move the cursor "backwards".
        cursor.advance(author, 0, 10);
        cursor.advance(author, 0, 5);
        assert_eq!(cursor.log_height(&author, &0), Some(&10));
    }

    #[test]
    fn compare() {
        let author = SigningKey::generate().verifying_key();
        let log_id_1 = 1;
        let log_id_2 = 2;

        let mut cursor_1 = Cursor::<VerifyingKey, u64>::new("one", LogHeights::default());
        let mut cursor_2 = Cursor::<VerifyingKey, u64>::new("two", LogHeights::default());

        cursor_1.advance(author, log_id_1, 121);
        cursor_1.advance(author, log_id_2, 13);
        cursor_2.advance(author, log_id_1, 287);

        let ranges = cursor_1.compare(cursor_2.state());
        assert_eq!(
            ranges.get(&author).unwrap().get(&log_id_1).unwrap(),
            &(Some(121), Some(287))
        );
        assert!(ranges.get(&author).unwrap().get(&log_id_2).is_none());

        let ranges = cursor_2.compare(cursor_1.state());
        assert!(ranges.get(&author).unwrap().get(&log_id_1).is_none());
        assert_eq!(
            ranges.get(&author).unwrap().get(&log_id_2).unwrap(),
            &(None, Some(13))
        );
    }
}
