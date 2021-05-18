use bamboo_rs_core::lipmaa;
use serde::{Deserialize, Serialize};

use crate::entry::SeqNumError;
use crate::Validate;

/// Start counting entries from here.
pub const FIRST_SEQ_NUM: i64 = 1;

/// Sequence number describing the position of an entry in its append-only log.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "db-sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct SeqNum(i64);

impl SeqNum {
    /// Validates and wraps value into a new `SeqNum` instance.
    ///
    /// ## Example
    ///
    /// ```
    /// # extern crate p2panda_rs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use p2panda_rs::entry::SeqNum;
    ///
    /// // Generate new sequence number
    /// let seq_num = SeqNum::new(2)?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(value: i64) -> Result<Self, SeqNumError> {
        let seq_num = Self(value);
        seq_num.validate()?;
        Ok(seq_num)
    }

    /// Return sequence number of the previous entry (backlink).
    ///
    /// ## Example
    ///
    /// ```
    /// # extern crate p2panda_rs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use p2panda_rs::entry::SeqNum;
    ///
    /// // Return backlink (sequence number of the previous entry)
    /// let seq_num = SeqNum::new(2)?;
    /// let backlink = seq_num.backlink_seq_num();
    ///
    /// assert_eq!(backlink, Some(SeqNum::new(1)?));
    /// # Ok(())
    /// # }
    /// ```
    pub fn backlink_seq_num(&self) -> Option<Self> {
        Self::new(self.0 - 1).ok()
    }

    /// Return sequence number of the lipmaa entry (skiplink).
    ///
    /// See [`Bamboo specification`] for more details about how skiplinks are calculated.
    ///
    /// [`Bamboo specification`]: https://github.com/AljoschaMeyer/bamboo#links-and-entry-verification
    pub fn skiplink_seq_num(&self) -> Option<Self> {
        Some(Self(lipmaa(self.0 as u64) as i64))
    }

    /// Returns true when sequence number marks first entry in log.
    pub fn is_first(&self) -> bool {
        self.0 == FIRST_SEQ_NUM
    }

    /// Returns `SeqNum` as i64 integer.
    pub fn as_i64(&self) -> i64 {
        self.0
    }
}

impl Default for SeqNum {
    fn default() -> Self {
        Self::new(FIRST_SEQ_NUM).unwrap()
    }
}

impl Copy for SeqNum {}

impl Validate for SeqNum {
    type Error = SeqNumError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Numbers have to be larger than zero
        if self.0 < FIRST_SEQ_NUM {
            return Err(SeqNumError::NotZeroOrNegative);
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
            SeqNum::new(4).unwrap()
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
