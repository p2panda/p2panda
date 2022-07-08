// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::hash::Hash as StdHash;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::entry::decode::StringOrU64;
use crate::entry::error::LogIdError;

/// Authors can write entries to multiple logs identified by log ids.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, StdHash)]
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

impl Iterator for LogId {
    type Item = LogId;

    fn next(&mut self) -> Option<Self::Item> {
        Some(Self(self.0 + 1))
    }
}

impl From<u64> for LogId {
    fn from(value: u64) -> Self {
        Self::new(value)
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

impl<'de> Deserialize<'de> for LogId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StringOrU64::<LogId>::new())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rstest::rstest;
    use serde::Serialize;

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
    fn string_conversions() {
        let large_number = "291919188205818203";
        let log_id_from_str: LogId = large_number.parse().unwrap();
        let log_id_try_from = LogId::try_from(String::from(large_number)).unwrap();
        assert_eq!(291919188205818203, log_id_from_str.as_u64());
        assert_eq!(log_id_from_str, log_id_try_from);
    }

    #[rstest]
    #[case("0", Some(LogId::new(0)))]
    #[case(12, Some(LogId::new(12)))]
    #[case("12", Some(LogId::new(12)))]
    #[case(u64::MAX, Some(LogId::new(u64::MAX)))]
    #[case("-12", None)]
    #[case("Not a log id", None)]
    fn deserialize_str_and_u64(
        #[case] value: impl Serialize + Sized,
        #[case] expected_result: Option<LogId>,
    ) {
        fn convert<T: Serialize + Sized>(value: T) -> Result<LogId, Box<dyn std::error::Error>> {
            let mut cbor_bytes = Vec::new();
            ciborium::ser::into_writer(&value, &mut cbor_bytes)?;
            let log_id: LogId = ciborium::de::from_reader(&cbor_bytes[..])?;
            Ok(log_id)
        }

        match expected_result {
            Some(result) => {
                assert_eq!(convert(value).unwrap(), result);
            }
            None => {
                assert!(convert(value).is_err());
            }
        }
    }
}
