/// Authors can write entries to multiple logs identified by log ids.
#[derive(Clone, Debug)]
pub struct LogId(u64);

impl LogId {
    /// Returns a new LogId instance.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns true when LogId is for a user schema.
    pub fn is_user_log(&self) -> bool {
        // Log ids for user schemas are odd numbers
        self.0 % 2 == 1
    }

    /// Returns true when LogId is for a system schema.
    pub fn is_system_log(&self) -> bool {
        // Log ids for system schemas are even numbers
        self.0 % 2 == 0
    }

    /// Returns `LogId` as u64 integer.
    pub fn as_u64(&self) -> u64 {
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
        assert_eq!(log_id.is_user_log(), true);
        assert_eq!(log_id.is_system_log(), false);

        let mut next_log_id = log_id.next().unwrap();
        assert_eq!(next_log_id, LogId::new(3));

        let next_log_id = next_log_id.next().unwrap();
        assert_eq!(next_log_id, LogId::new(5));
    }

    #[test]
    fn system_log_ids() {
        let mut log_id = LogId::new(0);
        assert_eq!(log_id.is_user_log(), false);
        assert_eq!(log_id.is_system_log(), true);

        // Can't iterate on system logs
        assert!(log_id.next().is_none());
    }
}
