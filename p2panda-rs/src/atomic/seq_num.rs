use anyhow::bail;
use bamboo_rs_core::lipmaa;
use thiserror::Error;

use crate::atomic::Validation;
use crate::Result;

/// Start counting entries from here.
pub const FIRST_SEQ_NUM: u64 = 1;

/// Custom error types for `SeqNum`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum SeqNumError {
    /// Sequence numbers are always positive.
    #[error("sequence number can not be zero or negative")]
    NotZeroOrNegative,
}

/// Sequence number describing the position of an entry in its append-only log.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "db-sqlx", derive(sqlx::Type))]
#[cfg_attr(feature = "db-sqlx", sqlx(transparent))]
pub struct SeqNum(u64);

impl SeqNum {
    /// Validates and wraps value into a new `SeqNum` instance.
    pub fn new(value: u64) -> Result<Self> {
        let seq_num = Self(value);
        seq_num.validate()?;
        Ok(seq_num)
    }

    /// Return sequence number of the previous entry (backlink).
    pub fn backlink_seq_num(&self) -> Option<Self> {
        Self::new(self.0 - 1).ok()
    }

    /// Return sequence number of the lipmaa entry (skiplink).
    ///
    /// See [`Bamboo specification`] for more details about how skiplinks are calculated.
    ///
    /// [`Bamboo specification`]: https://github.com/AljoschaMeyer/bamboo#links-and-entry-verification
    pub fn skiplink_seq_num(&self) -> Option<Self> {
        Some(Self(lipmaa(self.0) + FIRST_SEQ_NUM))
    }

    /// Returns true when sequence number marks first entry in log.
    pub fn is_first(&self) -> bool {
        self.0 == FIRST_SEQ_NUM
    }

    /// Returns `SeqNum` as u64 integer.
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Default for SeqNum {
    fn default() -> Self {
        Self::new(FIRST_SEQ_NUM).unwrap()
    }
}

impl Copy for SeqNum {}

impl Validation for SeqNum {
    fn validate(&self) -> Result<()> {
        // Numbers have to be larger than zero
        if self.0 < FIRST_SEQ_NUM {
            bail!(SeqNumError::NotZeroOrNegative)
        }

        Ok(())
    }
}

impl Iterator for SeqNum {
    type Item = SeqNum;

    fn next(&mut self) -> Option<Self::Item> {
        Some(Self(self.0 + 1))
    }
}

impl PartialEq for SeqNum {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[cfg(test)]
mod tests {
    use super::SeqNum;

    #[test]
    fn validate() {
        assert!(SeqNum::new(0).is_err());
        assert!(SeqNum::new(100).is_ok());
    }

    #[test]
    fn skiplink_seq_num() {
        assert_eq!(
            SeqNum::new(13).unwrap().skiplink_seq_num().unwrap(),
            SeqNum::new(5).unwrap()
        );
    }

    #[test]
    fn backlink_seq_num() {
        assert_eq!(
            SeqNum::new(12).unwrap().backlink_seq_num().unwrap(),
            SeqNum::new(11).unwrap()
        );

        assert!(SeqNum::new(1).unwrap().backlink_seq_num().is_none());
    }
}
