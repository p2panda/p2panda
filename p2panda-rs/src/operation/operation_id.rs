// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::hash::{Hash, HashId};
use crate::operation::error::OperationIdError;
use crate::{Human, Validate};

/// Uniquely identifies an [`Operation`](crate::operation::Operation).
///
/// An `OperationId` is the hash of the [`Entry`](crate::entry::Entry) with which an operation was
/// published.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialOrd, PartialEq, Serialize, Deserialize)]
pub struct OperationId(Hash);

impl OperationId {
    /// Returns an `OperationId` given an entry's hash.
    pub fn new(entry_hash: &Hash) -> Self {
        Self(entry_hash.to_owned())
    }

    /// Extracts a string slice from the operation id's hash.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl HashId for OperationId {
    /// Access the inner [`crate::hash::Hash`] value of this operation id.
    fn as_hash(&self) -> &Hash {
        &self.0
    }
}

impl Validate for OperationId {
    type Error = OperationIdError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()?;
        Ok(())
    }
}

impl From<Hash> for OperationId {
    fn from(hash: Hash) -> Self {
        Self::new(&hash)
    }
}

impl FromStr for OperationId {
    type Err = OperationIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Hash::new(s)?))
    }
}

impl TryFrom<String> for OperationId {
    type Error = OperationIdError;

    fn try_from(str: String) -> Result<Self, Self::Error> {
        Self::from_str(&str)
    }
}

impl Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Human for OperationId {
    fn display(&self) -> String {
        let offset = blake3::KEY_LEN * 2 - 6;
        format!("<Operation {}>", &self.0.as_str()[offset..])
    }
}
