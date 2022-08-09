// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::fmt::Display;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Operations are categorised by their action type.
///
/// An action defines the operation format and if this operation creates, updates or deletes a data
/// document.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OperationAction {
    /// Operation creates a new document.
    Create,

    /// Operation updates an existing document.
    Update,

    /// Operation deletes an existing document.
    Delete,
}

impl OperationAction {
    /// Returns the operation action as a string.
    pub fn as_str(&self) -> &str {
        match self {
            OperationAction::Create => "create",
            OperationAction::Update => "update",
            OperationAction::Delete => "delete",
        }
    }

    /// Returns the operation action encoded as u64.
    pub fn as_u64(&self) -> u64 {
        match self {
            OperationAction::Create => 0,
            OperationAction::Update => 1,
            OperationAction::Delete => 2,
        }
    }
}

impl Serialize for OperationAction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.as_u64())
    }
}

impl<'de> Deserialize<'de> for OperationAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let action = u64::deserialize(deserializer)?;

        match action {
            0 => Ok(OperationAction::Create),
            1 => Ok(OperationAction::Update),
            2 => Ok(OperationAction::Delete),
            _ => Err(serde::de::Error::custom(format!(
                "unknown operation action {}",
                action
            ))),
        }
    }
}

impl Display for OperationAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::OperationAction;

    #[test]
    fn string_representation() {
        assert_eq!(OperationAction::Create.as_str(), "create");
        assert_eq!(OperationAction::Update.as_str(), "update");
        assert_eq!(OperationAction::Delete.as_str(), "delete");

        assert_eq!(format!("{}", OperationAction::Create), "create");
        assert_eq!(format!("{}", OperationAction::Update), "update");
        assert_eq!(format!("{}", OperationAction::Delete), "delete");
    }

    #[test]
    fn as_u64() {
        assert_eq!(OperationAction::Create.as_u64(), 0);
        assert_eq!(OperationAction::Update.as_u64(), 1);
        assert_eq!(OperationAction::Delete.as_u64(), 2);
    }
}
