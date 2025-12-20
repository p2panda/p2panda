// SPDX-License-Identifier: MIT OR Apache-2.0

use std::str::FromStr;

use iroh::discovery::UserData;
use iroh::endpoint_info::MaxLengthExceededError;
use p2panda_core::{IdentityError, Signature};
use thiserror::Error;

use crate::timestamp::{HybridTimestamp, HybridTimestampError};
use crate::{AuthenticatedTransportInfo, NodeTransportInfo, TransportAddress};

/// Helper to bring additional transport info (signature and timestamp) into iroh's user data
/// struct.
///
/// We need this data to check the authenticity of the transport info.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserDataTransportInfo {
    pub signature: Signature,
    pub timestamp: HybridTimestamp,
    // TODO: We're including the endpoints "home relay url" in the user data as well as this is
    // currently not supported by iroh for mDNS discovery.
    //
    // Without the relay url being part of the transport info we would break the signature.
    //
    // See related issue: https://github.com/n0-computer/iroh/issues/3682
    pub relay_url: Option<iroh::RelayUrl>,
}

impl UserDataTransportInfo {
    pub fn from_transport_info(info: AuthenticatedTransportInfo) -> Self {
        Self {
            signature: info.signature,
            timestamp: info.timestamp,
            relay_url: info.addresses().iter().find_map(|addr| match addr {
                TransportAddress::Iroh(addr) => addr.relay_urls().next().cloned(),
            }),
        }
    }
}

impl TryFrom<AuthenticatedTransportInfo> for UserData {
    type Error = MaxLengthExceededError;

    fn try_from(info: AuthenticatedTransportInfo) -> Result<Self, Self::Error> {
        UserData::try_from(UserDataTransportInfo::from_transport_info(info))
    }
}

const INFO_SEPARATOR: char = '|';

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
            "{}{INFO_SEPARATOR}{}{}",
            info.signature,
            info.timestamp,
            info.relay_url
                .map(|url| format!("{INFO_SEPARATOR}{url}"))
                .unwrap_or_default()
        ))
    }
}

impl TryFrom<UserData> for UserDataTransportInfo {
    type Error = UserDataInfoError;

    fn try_from(user_data: UserData) -> Result<Self, Self::Error> {
        let user_data = user_data.to_string();

        // Try to split string by separator into two halfs.
        let parts: Vec<_> = user_data.split(INFO_SEPARATOR).collect();
        if parts.len() != 2 && parts.len() != 3 {
            return Err(UserDataInfoError::Size(parts.len()));
        }

        let mut parts = parts.iter();
        let signature_str = parts.next().expect("we've checked the size before");
        let timestamp_str = parts.next().expect("we've checked the size before");

        // Try to parse halfs into signature and timestamp.
        let signature = Signature::from_str(signature_str)?;
        let timestamp = HybridTimestamp::from_str(timestamp_str)?;

        // Try to parse optional relay url.
        let relay_url = if let Some(relay_url_str) = parts.next() {
            Some(iroh::RelayUrl::from_str(relay_url_str)?)
        } else {
            None
        };

        Ok(Self {
            signature,
            timestamp,
            relay_url,
        })
    }
}

#[derive(Debug, Error)]
pub enum UserDataInfoError {
    #[error("invalid size of separated info parts, expected 2-3, given: {0}")]
    Size(usize),

    #[error(transparent)]
    Signature(#[from] IdentityError),

    #[error(transparent)]
    Timestamp(#[from] HybridTimestampError),

    #[error(transparent)]
    RelayUrl(#[from] iroh::RelayUrlParseError),
}

#[cfg(test)]
mod tests {
    use iroh::discovery::UserData;
    use p2panda_core::PrivateKey;

    use crate::utils::from_public_key;

    use super::{AuthenticatedTransportInfo, UserDataTransportInfo};

    #[test]
    fn transport_info_to_user_data() {
        // Create simple transport info object without any addresses attached.
        let private_key = PrivateKey::new();
        let transport_info = AuthenticatedTransportInfo::new_unsigned()
            .sign(&private_key)
            .unwrap();

        // Extract information we want for our TXT record.
        let txt_info = UserDataTransportInfo::from_transport_info(transport_info);

        // Convert it into iroh data type.
        let user_data = UserData::try_from(txt_info.clone()).unwrap();

        // .. and back!
        let txt_info_again = UserDataTransportInfo::try_from(user_data).unwrap();
        assert_eq!(txt_info, txt_info_again);
    }

    #[test]
    fn transport_info_to_user_data_with_relay_url() {
        let private_key = PrivateKey::new();
        let mut transport_info = AuthenticatedTransportInfo::new_unsigned();
        transport_info.add_addr(
            iroh::EndpointAddr::new(from_public_key(private_key.public_key()))
                .with_ip_addr("127.0.0.1:8080".parse().unwrap())
                .with_relay_url(
                    "https://euc1-1.relay.n0.iroh-canary.iroh.link./"
                        .parse()
                        .unwrap(),
                )
                .into(),
        );
        let transport_info = transport_info.sign(&private_key).unwrap();

        // Extract information we want for our TXT record.
        let txt_info = UserDataTransportInfo::from_transport_info(transport_info);

        // Convert it into iroh data type.
        let user_data = UserData::try_from(txt_info.clone()).unwrap();

        // .. and back!
        let txt_info_again = UserDataTransportInfo::try_from(user_data).unwrap();
        assert_eq!(txt_info, txt_info_again);
    }
}
