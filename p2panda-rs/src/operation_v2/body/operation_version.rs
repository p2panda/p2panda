// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Operation format versions to introduce API changes in the future.
///
/// Operations contain the actual data of applications in the p2panda network and will be stored
/// for an indefinite time on different machines. To allow an upgrade path in the future and
/// support backwards compatibility for old data we can use this version number.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OperationVersion {
    /// The latest version number.
    V1,
}

impl OperationVersion {
    /// Returns the operation version encoded as u64.
    pub fn as_u64(&self) -> u64 {
        match self {
            OperationVersion::V1 => 1,
        }
    }
}

impl Serialize for OperationVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.as_u64())
    }
}

impl<'de> Deserialize<'de> for OperationVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version = u64::deserialize(deserializer)?;

        match version {
            1 => Ok(OperationVersion::V1),
            _ => Err(serde::de::Error::custom(format!(
                "unsupported operation version {}",
                version
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;

    use crate::serde::{deserialize_into, serialize_from, serialize_value};

    use super::OperationVersion;

    #[test]
    fn u64_representation() {
        assert_eq!(OperationVersion::V1.as_u64(), 1);
    }

    #[test]
    fn serialize() {
        let bytes = serialize_from(OperationVersion::V1);
        assert_eq!(bytes, vec![1]);
    }

    #[test]
    fn deserialize() {
        let version: OperationVersion = deserialize_into(&serialize_value(cbor!(1))).unwrap();
        assert_eq!(version, OperationVersion::V1);

        // Unsupported version number
        let invalid_version = deserialize_into::<OperationVersion>(&serialize_value(cbor!(0)));
        assert!(invalid_version.is_err());

        // Can not be a string
        let invalid_type = deserialize_into::<OperationVersion>(&serialize_value(cbor!("0")));
        assert!(invalid_type.is_err());
    }
}
