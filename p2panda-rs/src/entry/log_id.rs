// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

/// Authors can write entries to multiple logs identified by log ids.
///
/// While the Bamboo spec calls for unsigned 64 bit integers, these are not supported by SQLite,
/// which we want to support. Therefore, we use `i64` here.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(
    feature = "db-sqlx",
    derive(sqlx::Type, sqlx::FromRow),
    sqlx(transparent)
)]
pub struct LogId(i64);

impl LogId {
    /// Validates and wraps log id value into a new `LogId` instance.
    pub fn new(value: i64) -> Self {
        Self(value)
    }

    /// Returns true when `LogId` is for a user schema.
    pub fn is_user_log(&self) -> bool {
        // Log ids for user schemas are odd numbers
        self.0 % 2 == 1
    }

    /// Returns true when `LogId` is for a system schema.
    pub fn is_system_log(&self) -> bool {
        // Log ids for system schemas are even numbers
        self.0 % 2 == 0
    }

    /// Returns `LogId` as i64 integer.
    pub fn as_i64(&self) -> i64 {
        self.0
    }
}

impl Default for LogId {
    fn default() -> Self {
        // Log ids for system schemes are defined by the specification and fixed, the default value
        // is hence the first possible user schema log id.
        Self::new(1)
    }
}

impl Copy for LogId {}

impl Iterator for LogId {
    type Item = LogId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_user_log() {
            Some(Self(self.0 + 2))
        } else {
            None
        }
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
    fn user_log_ids() {
        let mut log_id = LogId::default();
        assert!(log_id.is_user_log());
        assert!(!log_id.is_system_log());

        let mut next_log_id = log_id.next().unwrap();
        assert_eq!(next_log_id, LogId::new(3));

        let next_log_id = next_log_id.next().unwrap();
        assert_eq!(next_log_id, LogId::new(5));
    }

    #[test]
    fn system_log_ids() {
        let mut log_id = LogId::new(0);
        assert!(!log_id.is_user_log());
        assert!(log_id.is_system_log());

        // Can't iterate on system logs
        assert!(log_id.next().is_none());
    }
}
