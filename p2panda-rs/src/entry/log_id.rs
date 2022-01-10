// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::entry::error::LogIdError;

/// Authors can write entries to multiple logs identified by log ids.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "&str")]
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

impl fmt::Display for LogId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for LogId {
    type Err = LogIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LogId::try_from(s)
    }
}

/// Convert any string representation of an u64 integer into an `LogId` instance.
impl TryFrom<&str> for LogId {
    type Error = LogIdError;

    fn try_from(str: &str) -> Result<Self, Self::Error> {
        Ok(Self::new(
            u64::from_str(str).map_err(|_| LogIdError::InvalidU64String)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use super::LogId;

    #[test]
    fn log_ids() {
        let mut log_id = LogId::default();

        let mut next_log_id = log_id.next().unwrap();
        assert_eq!(next_log_id, LogId::new(2));

        let next_log_id = next_log_id.next().unwrap();
        assert_eq!(next_log_id, LogId::new(3));
    }

    #[test]
    fn u64_conversion() {
        let large_number = "291919188205818203";
        let log_id = LogId::try_from(large_number).unwrap();
        assert_eq!(291919188205818203, log_id.as_u64());
    }
}
