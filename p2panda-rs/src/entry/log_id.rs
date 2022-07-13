// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::hash::Hash as StdHash;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::entry::error::LogIdError;

/// Authors can write entries to multiple logs identified by log ids.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, StdHash)]
pub struct LogId(u64);

impl LogId {
    /// Returns a new `LogId` instance.
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
        Self::new(0)
    }
}

impl Iterator for LogId {
    type Item = LogId;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0 == std::u64::MAX {
            true => None,
            false => {
                self.0 += 1;
                Some(*self)
            }
        }
    }
}

/// Convert any borrowed string representation of an u64 integer into an `LogId` instance.
impl FromStr for LogId {
    type Err = LogIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(
            u64::from_str(s).map_err(|_| LogIdError::InvalidU64String)?,
        ))
    }
}

/// Convert any owned string representation of an u64 integer into an `LogId` instance.
impl TryFrom<String> for LogId {
    type Error = LogIdError;

    fn try_from(str: String) -> Result<Self, Self::Error> {
        Ok(Self::new(
            u64::from_str(&str).map_err(|_| LogIdError::InvalidU64String)?,
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
        assert_eq!(next_log_id, LogId::new(1));

        let next_log_id = next_log_id.next().unwrap();
        assert_eq!(next_log_id, LogId::new(2));
    }

    #[test]
    fn iterator() {
        let mut log_id = LogId::default();

        assert_eq!(Some(LogId(1)), log_id.next());
        assert_eq!(Some(LogId(2)), log_id.next());
        assert_eq!(Some(LogId(3)), log_id.next());

        let mut log_id = LogId(std::u64::MAX - 1);

        assert_eq!(Some(LogId(std::u64::MAX)), log_id.next());
        assert_eq!(None, log_id.next());
    }

    #[test]
    fn string_conversions() {
        let large_number = "291919188205818203";
        let log_id_from_str: LogId = large_number.parse().unwrap();
        let log_id_try_from = LogId::try_from(String::from(large_number)).unwrap();
        assert_eq!(291919188205818203, log_id_from_str.as_u64());
        assert_eq!(log_id_from_str, log_id_try_from);
    }
}
