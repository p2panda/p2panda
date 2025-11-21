// SPDX-License-Identifier: MIT OR Apache-2.0

use std::num::ParseIntError;
use std::str::FromStr;

use iroh::discovery::UserData;
use iroh::endpoint_info::MaxLengthExceededError;
use p2panda_core::{IdentityError, Signature};
use thiserror::Error;
use tracing::error;

use crate::TransportInfo;

/// Helper to bring additional transport info (signature and timestamp) into iroh's user data
/// struct.
///
/// We need this data to check the authenticity of the transport info.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserDataTransportInfo {
    pub signature: Signature,
    pub timestamp: u64,
}

impl UserDataTransportInfo {
    pub fn from_transport_info(info: TransportInfo) -> Self {
        Self {
            signature: info.signature,
            timestamp: info.timestamp,
        }
    }
}

impl TryFrom<TransportInfo> for UserData {
    type Error = MaxLengthExceededError;

    fn try_from(info: TransportInfo) -> Result<Self, Self::Error> {
        UserData::try_from(UserDataTransportInfo::from_transport_info(info))
    }
}

const INFO_SEPARATOR: char = '.';

impl TryFrom<UserDataTransportInfo> for UserData {
    type Error = MaxLengthExceededError;

    fn try_from(info: UserDataTransportInfo) -> Result<Self, Self::Error> {
        // Encode the signature as an hex-string (128 characters) and the timestamp as a plain
        // number. There's a 245 character limit for iroh's user data due to the limit of DNS TXT
        // records.
        //
        // NOTE: This will currently fail if the u64 integer gets too large .. we can't "remote
        // crash" nodes because of that at least.
        UserData::try_from(format!(
            "{}{INFO_SEPARATOR}{}",
            info.signature, info.timestamp
        ))
    }
}

impl TryFrom<UserData> for UserDataTransportInfo {
    type Error = UserDataInfoError;

    fn try_from(user_data: UserData) -> Result<Self, Self::Error> {
        let user_data = user_data.to_string();

        // Try to split string by separator into two halfs.
        let parts: Vec<_> = user_data.split(INFO_SEPARATOR).collect();
        if parts.len() != 2 {
            return Err(UserDataInfoError::Size(parts.len()));
        }

        let mut parts = parts.iter();
        let signature_str = parts.next().expect("we've checked the size before");
        let timestamp_str = parts.next().expect("we've checked the size before");

        // Try to parse halfs into signature and timestamp.
        let signature = Signature::from_str(signature_str)?;
        let timestamp = u64::from_str(timestamp_str)?;

        Ok(Self {
            signature,
            timestamp,
        })
    }
}

#[derive(Debug, Error)]
pub enum UserDataInfoError {
    #[error("invalid size of separated info parts, expected 2, given: {0}")]
    Size(usize),

    #[error(transparent)]
    Signature(#[from] IdentityError),

    #[error(transparent)]
    Timestamp(#[from] ParseIntError),
}

#[cfg(test)]
mod tests {
    use iroh::discovery::UserData;
    use p2panda_core::PrivateKey;

    use super::{TransportInfo, UserDataTransportInfo};

    #[test]
    fn transport_info_to_user_data() {
        // Create simple transport info object without any addresses attached.
        let private_key = PrivateKey::new();
        let transport_info = TransportInfo::new_unsigned().sign(&private_key).unwrap();

        // Extract information we want for our TXT record.
        let txt_info = UserDataTransportInfo::from_transport_info(transport_info);

        // Convert it into iroh data type.
        let user_data = UserData::try_from(txt_info.clone()).unwrap();

        // .. and back!
        let txt_info_again = UserDataTransportInfo::try_from(user_data).unwrap();
        assert_eq!(txt_info, txt_info_again);
    }
}
