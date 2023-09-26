// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::fmt::Display;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::operation_v2::header::error::HeaderActionError;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HeaderAction {
    /// Operation deletes an existing document.
    Delete,
}

impl HeaderAction {
    /// Returns the operation action as a string.
    pub fn as_str(&self) -> &str {
        match self {
            HeaderAction::Delete => "delete",
        }
    }

    /// Returns the operation action encoded as u64.
    pub fn as_u64(&self) -> u64 {
        match self {
            HeaderAction::Delete => 1,
        }
    }
}

impl TryFrom<u64> for HeaderAction {
    type Error = HeaderActionError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            2 => Ok(HeaderAction::Delete),
            _ => Err(HeaderActionError::UnknownAction(value)),
        }
    }
}

impl Serialize for HeaderAction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.as_u64())
    }
}

impl<'de> Deserialize<'de> for HeaderAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let action = u64::deserialize(deserializer)?;

        action
            .try_into()
            .map_err(|err| serde::de::Error::custom(format!("{}", err)))
    }
}

impl Display for HeaderAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
