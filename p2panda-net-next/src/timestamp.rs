// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;
use std::hash::Hash as StdHash;
use std::num::ParseIntError;
use std::ops::Add;
use std::str::FromStr;
#[cfg(not(test))]
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

#[cfg(test)]
use mock_instant::SystemTimeError;
#[cfg(test)]
use mock_instant::thread_local::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Microseconds since the UNIX epoch based on system time.
///
/// This is using microseconds instead leap seconds for larger precision (unlike standard UNIX
/// timestamps).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, StdHash, Serialize, Deserialize)]
pub struct Timestamp(u64);

impl Timestamp {
    #[cfg(test)]
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn now() -> Self {
        let now = SystemTime::now();
        now.try_into().expect("system time went backwards")
    }
}

impl From<Timestamp> for u64 {
    fn from(value: Timestamp) -> Self {
        value.0
    }
}

#[cfg(test)]
impl From<u64> for Timestamp {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl TryFrom<SystemTime> for Timestamp {
    type Error = SystemTimeError;

    fn try_from(system_time: SystemTime) -> Result<Self, Self::Error> {
        let duration = system_time.duration_since(UNIX_EPOCH)?;
        // Use microseconds precision instead of seconds unlike standard UNIX timestamps.
        Ok(Self(duration.as_micros() as u64))
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Logical clock algorithm to determine the order of events.
///
/// <https://en.wikipedia.org/wiki/Lamport_timestamp>
#[derive(
    Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, StdHash, Serialize, Deserialize,
)]
pub struct LamportTimestamp(u64);

impl LamportTimestamp {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn increment(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Display for LamportTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Add<u64> for LamportTimestamp {
    type Output = LamportTimestamp;

    fn add(self, rhs: u64) -> Self::Output {
        Self(self.0.saturating_add(rhs))
    }
}

impl From<u64> for LamportTimestamp {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// Hybrid UNIX and logical clock timestamp.
///
/// This allows for settings where we want the guarantees of a monotonically incrementing lamport
/// timestamp but still "move forwards" with "global time" so we get the best of both worlds during
/// ordering:
///
/// * If we lost the state of our logical clock we will still be _after_ previous timestamps, as
/// the global UNIX time advanced (given that no OS clock was faulty).
/// * If the UNIX timestamp is the same we know which item came after because of the logical clock
/// and don't need to rely on more "random" tie-breakers, like a hashing digest.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, StdHash, Serialize, Deserialize)]
pub struct HybridTimestamp(Timestamp, LamportTimestamp);

impl HybridTimestamp {
    pub fn from_parts(timestamp: Timestamp, logical: LamportTimestamp) -> Self {
        Self(timestamp, logical)
    }

    pub fn now() -> Self {
        Self(Timestamp::now(), LamportTimestamp::default())
    }

    pub fn increment(self) -> Self {
        let timestamp = Timestamp::now();
        if timestamp == self.0 {
            Self(timestamp, self.1.increment())
        } else {
            Self(timestamp, LamportTimestamp::default())
        }
    }
}

const SEPARATOR: char = '/';

impl FromStr for HybridTimestamp {
    type Err = HybridTimestampError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = s.split(SEPARATOR).collect();
        if parts.len() != 2 {
            return Err(HybridTimestampError::Size(parts.len()));
        }

        let unix: u64 = u64::from_str(parts[0]).map_err(HybridTimestampError::ParseInt)?;
        let logical: u64 = u64::from_str(parts[1]).map_err(HybridTimestampError::ParseInt)?;

        Ok(Self(Timestamp(unix), LamportTimestamp::new(logical)))
    }
}

impl Display for HybridTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{SEPARATOR}{}", self.0, self.1)
    }
}

#[cfg(test)]
impl From<u64> for HybridTimestamp {
    fn from(value: u64) -> Self {
        Self(Timestamp::new(value), LamportTimestamp::default())
    }
}

#[derive(Debug, Error)]
pub enum HybridTimestampError {
    #[error("invalid size, expected 2, given: {0}")]
    Size(usize),

    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, time::Duration};

    use mock_instant::thread_local::MockClock;

    use super::{HybridTimestamp, LamportTimestamp};

    #[test]
    fn convert_and_compare() {
        assert!(LamportTimestamp(5) > 3.into());
    }

    #[test]
    fn add_u64_with_max() {
        assert_eq!(LamportTimestamp(3) + 3u64, LamportTimestamp(6));
        assert_eq!(
            LamportTimestamp(u64::MAX) + 3u64,
            LamportTimestamp(u64::MAX)
        );
    }

    #[test]
    fn increment_hybrid() {
        MockClock::set_system_time(Duration::from_secs(0));

        let timestamp_1 = HybridTimestamp::now();
        let timestamp_2 = timestamp_1.increment();
        assert!(timestamp_2 > timestamp_1);

        MockClock::advance_system_time(Duration::from_secs(1));

        let timestamp_3 = HybridTimestamp::now();
        let timestamp_4 = timestamp_3.increment();
        assert!(timestamp_3 > timestamp_2);
        assert!(timestamp_4 > timestamp_3);

        MockClock::advance_system_time(Duration::from_secs(1));

        let timestamp_5 = HybridTimestamp::now();
        let timestamp_6 = HybridTimestamp::now();

        assert!(timestamp_5 > timestamp_4);
        assert!(timestamp_6 > timestamp_4);
        assert_eq!(timestamp_5, timestamp_6);
    }

    #[test]
    fn hybrid_from_str() {
        let timestamp = HybridTimestamp::now().increment().increment();
        let timestamp_str = timestamp.to_string();
        assert_eq!(
            HybridTimestamp::from_str(&timestamp_str).unwrap(),
            timestamp
        );
    }
}
