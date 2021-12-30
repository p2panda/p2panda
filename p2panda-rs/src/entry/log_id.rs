// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

/// Authors can write entries to multiple logs identified by log ids.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogId(u64);

impl LogId {
    /// Validates and wraps log id value into a new `LogId` instance.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns `LogId` as u64 integer.
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Default for LogId {
    fn default() -> Self {
        Self::new(1)
    }
}

impl Copy for LogId {}

impl Iterator for LogId {
    type Item = LogId;

    fn next(&mut self) -> Option<Self::Item> {
        Some(Self(self.0 + 1))
    }
}

impl PartialEq for LogId {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[cfg(test)]
mod tests {
    use super::LogId;

    #[test]
    fn log_ids() {
        let mut log_id = LogId::default();

        let mut next_log_id = log_id.next().unwrap();
        assert_eq!(next_log_id, LogId::new(2));

        let next_log_id = next_log_id.next().unwrap();
        assert_eq!(next_log_id, LogId::new(3));
    }
}
