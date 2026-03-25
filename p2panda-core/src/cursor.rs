// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;

use crate::identity::Author;
use crate::logs::{LogHeights, LogId, LogRanges, SeqNum, compare};

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

    pub fn state(&self) -> &LogHeights<A, L> {
        &self.state
    }

    pub fn compare(&self, other: &LogHeights<A, L>) -> LogRanges<A, L> {
        compare(other, &self.state)
    }

    pub fn log_height(&self, author: &A, log_id: &L) -> Option<&SeqNum> {
        self.state.get(author).and_then(|logs| logs.get(log_id))
    }

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
