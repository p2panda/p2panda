// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Default lifetime which amounts to 3 * 28 Days, i.e. about 3 months.
const DEFAULT_LIFETIME: u64 = 60 * 60 * 24 * 28 * 3;

/// The lifetime is extended into the past to allow for skewed clocks. The value is in seconds and
/// amounts to 1h.
const DEFAULT_LIFETIME_MARGIN: u64 = 60 * 60;

/// Determines the lifetime of a pre-key by defining a time range.
///
/// Implementations should reject received messages with expired pre-keys and disallow use.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lifetime {
    not_before: u64,
    not_after: u64,
}

impl Lifetime {
    /// Create a new lifetime in seconds from now on.
    ///
    /// Note that the lifetime is extended 1h into the past to adapt to skewed clocks, i.e.
    /// `not_before` is set to `now - 1h`.
    pub fn new(t: u64) -> Self {
        let lifetime_margin: u64 = DEFAULT_LIFETIME_MARGIN;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH!")
            .as_secs();
        let not_before = now - lifetime_margin;
        let not_after = now + t;
        Self {
            not_before,
            not_after,
        }
    }

    /// Returns true if this lifetime is valid.
    pub fn verify(&self) -> Result<(), LifetimeError> {
        let is_valid = match SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
        {
            Ok(elapsed) => self.not_before < elapsed && elapsed < self.not_after,
            Err(err) => return Err(LifetimeError::SystemTime(err)),
        };

        if is_valid {
            Ok(())
        } else {
            Err(LifetimeError::InvalidLifetime)
        }
    }

    /// Returns true if this lifetime with an additional window is valid.
    ///
    /// This method can be used to calculate if a lifetime _is about_ to expire, given a defined
    /// time "window".
    ///
    /// ```plain
    ///               [-------]
    ///               |       |
    /// Lifetime: [---|----]  |
    ///               |       |
    ///              now  + window
    ///
    ///         -- t -->
    /// ```
    pub fn verify_with_window(&self, window: Duration) -> Result<(), LifetimeError> {
        let is_valid = match SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
        {
            Ok(elapsed) => {
                let elapsed = elapsed + window.as_secs();
                self.not_before < elapsed && elapsed < self.not_after
            }
            Err(err) => return Err(LifetimeError::SystemTime(err)),
        };

        if is_valid {
            Ok(())
        } else {
            Err(LifetimeError::InvalidLifetime)
        }
    }
}

impl Default for Lifetime {
    fn default() -> Self {
        Lifetime::new(DEFAULT_LIFETIME)
    }
}

impl Ord for Lifetime {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.not_after.cmp(&other.not_after)
    }
}

impl PartialOrd for Lifetime {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl Lifetime {
    pub fn from_range(not_before: u64, not_after: u64) -> Self {
        Self {
            not_before,
            not_after,
        }
    }
}

#[derive(Debug, Error)]
pub enum LifetimeError {
    #[error("lifetime of pre-key is not valid")]
    InvalidLifetime,

    #[error(transparent)]
    SystemTime(std::time::SystemTimeError),
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::Lifetime;

    #[test]
    fn verify() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH!")
            .as_secs();

        // Default lifetimes are correct.
        let lifetime = Lifetime::default();
        assert!(lifetime.verify().is_ok());

        // Lifetimes of a minute are correct.
        let lifetime = Lifetime::new(60);
        assert!(lifetime.verify().is_ok());
        assert!(
            lifetime
                .verify_with_window(Duration::from_secs(120))
                .is_err()
        );

        // Test interface for lifetime is correct.
        let lifetime = Lifetime::from_range(now - 60, now + 60);
        assert!(lifetime.verify().is_ok());

        // Invalid lifetimes throw an error.
        let lifetime = Lifetime::from_range(now + 60, now + 60); // too early
        assert!(lifetime.verify().is_err());

        let lifetime = Lifetime::from_range(now - 120, now - 60); // too late
        assert!(lifetime.verify().is_err());
    }
}
