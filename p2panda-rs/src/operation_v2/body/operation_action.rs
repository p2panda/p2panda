// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;
use std::fmt::Display;
use std::{convert::TryFrom, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::operation::error::OperationActionError;

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

impl TryFrom<u64> for OperationAction {
    type Error = OperationActionError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(OperationAction::Create),
            1 => Ok(OperationAction::Update),
            2 => Ok(OperationAction::Delete),
            _ => Err(OperationActionError::UnknownAction(value)),
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

        action
            .try_into()
            .map_err(|err| serde::de::Error::custom(format!("{}", err)))
    }
}

impl Display for OperationAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use ciborium::cbor;

    use crate::serde::{deserialize_into, serialize_from, serialize_value};

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

    #[test]
    fn from_u64() {
        let create = OperationAction::try_from(0);
        matches!(create, Ok(OperationAction::Create));
        let update = OperationAction::try_from(1);
        matches!(update, Ok(OperationAction::Update));
        let delete = OperationAction::try_from(2);
        matches!(delete, Ok(OperationAction::Delete));

        let invalid = OperationAction::try_from(12);
        assert!(invalid.is_err());
    }

    #[test]
    fn serialize() {
        let bytes = serialize_from(OperationAction::Create);
        assert_eq!(bytes, vec![0]);

        let bytes = serialize_from(OperationAction::Delete);
        assert_eq!(bytes, vec![2]);
    }

    #[test]
    fn deserialize() {
        let action: OperationAction = deserialize_into(&serialize_value(cbor!(1))).unwrap();
        assert_eq!(action, OperationAction::Update);

        // Unsupported action
        let invalid_action = deserialize_into::<OperationAction>(&serialize_value(cbor!(12)));
        assert!(invalid_action.is_err());

        // Can not be a string
        let invalid_type = deserialize_into::<OperationAction>(&serialize_value(cbor!("0")));
        assert!(invalid_type.is_err());
    }
}
