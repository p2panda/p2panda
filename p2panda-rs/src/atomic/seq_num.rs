use bamboo_rs_core::lipmaa;
use validator::{Validate, ValidationError, ValidationErrors};

use crate::error::{Result, ValidationResult};

/// Start counting entries from here.
pub const FIRST_SEQ_NUM: i64 = 1;

/// Sequence number describing the position of an entry in its append-only log.
///
/// By specification the `seq_num` is an u64 integer but since this is not supported by sqlx we use
/// the signed variant i64.
#[derive(Clone, Debug)]
pub struct SeqNum(i64);

impl SeqNum {
    /// Validates and returns a new sequence number when correct.
    pub fn new(value: i64) -> Result<Self> {
        let seq_num = Self(value);
        seq_num.validate()?;
        Ok(seq_num)
    }

    /// Return sequence number of the previous entry (backlink).
    #[allow(dead_code)]
    pub fn backlink_seq_num(&self) -> Result<Self> {
        Self::new(self.0 - 1)
    }

    /// Return sequence number of the lipmaa entry (skiplink).
    ///
    /// See Bamboo specification for more details about how skiplinks are calculated.
    pub fn skiplink_seq_num(&self) -> Self {
        Self(lipmaa(self.0 as u64) as i64 + FIRST_SEQ_NUM)
    }
}

impl Default for SeqNum {
    fn default() -> Self {
        Self::new(FIRST_SEQ_NUM).unwrap()
    }
}

impl Copy for SeqNum {}

impl Validate for SeqNum {
    fn validate(&self) -> ValidationResult {
        let mut errors = ValidationErrors::new();

        // Numbers have to be larger than zero
        if self.0 < FIRST_SEQ_NUM {
            errors.add("logId", ValidationError::new("can't be zero or negative"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
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
        assert!(SeqNum::new(-1).is_err());
        assert!(SeqNum::new(0).is_err());
        assert!(SeqNum::new(100).is_ok());
    }

    #[test]
    fn skiplink_seq_num() {
        assert_eq!(
            SeqNum::new(13).unwrap().skiplink_seq_num(),
            SeqNum::new(5).unwrap()
        );
    }

    #[test]
    fn backlink_seq_num() {
        assert_eq!(
            SeqNum::new(12).unwrap().backlink_seq_num().unwrap(),
            SeqNum::new(11).unwrap()
        );

        assert!(SeqNum::new(1).unwrap().backlink_seq_num().is_err());
    }
}
