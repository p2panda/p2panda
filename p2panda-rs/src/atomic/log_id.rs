use validator::{Validate, ValidationError, ValidationErrors};

use crate::error::{Result, ValidationResult};

/// Authors can write entries to multiple logs identified by log ids.
///
/// By specification the `log_id` is an u64 integer but since this is not supported by sqlx we use
/// the signed variant i64.
#[derive(Clone, Debug)]
pub struct LogId(i64);

impl LogId {
    /// Validates and returns a new LogId instance when correct.
    pub fn new(value: i64) -> Result<Self> {
        let log_id = Self(value);
        log_id.validate()?;
        Ok(log_id)
    }

    /// Returns true when LogId is for a user schema.
    pub fn is_user_log(&self) -> bool {
        // Log ids for user schemas are odd numbers
        self.0 % 2 == 1
    }

    /// Returns true when LogId is for a system schema.
    #[allow(dead_code)]
    pub fn is_system_log(&self) -> bool {
        // Log ids for system schemas are even numbers
        self.0 % 2 == 0
    }
}

impl Default for LogId {
    fn default() -> Self {
        // Log ids for system schemes are defined by the specification and fixed, the default value
        // is hence the first possible user schema log id.
        Self::new(1).unwrap()
    }
}

impl Copy for LogId {}

impl Validate for LogId {
    fn validate(&self) -> ValidationResult {
        let mut errors = ValidationErrors::new();

        // Numbers have to be positive
        if self.0 < 0 {
            errors.add("logId", ValidationError::new("can't be negative"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

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
    fn validate() {
        assert!(LogId::new(-1).is_err());
        assert!(LogId::new(100).is_ok());
    }

    #[test]
    fn user_log_ids() {
        let mut log_id = LogId::default();
        assert_eq!(log_id.is_user_log(), true);
        assert_eq!(log_id.is_system_log(), false);

        let mut next_log_id = log_id.next().unwrap();
        assert_eq!(next_log_id, LogId::new(3).unwrap());

        let next_log_id = next_log_id.next().unwrap();
        assert_eq!(next_log_id, LogId::new(5).unwrap());
    }

    #[test]
    fn system_log_ids() {
        let mut log_id = LogId::new(0).unwrap();
        assert_eq!(log_id.is_user_log(), false);
        assert_eq!(log_id.is_system_log(), true);

        // Can't iterate on system logs
        assert!(log_id.next().is_none());
    }
}
