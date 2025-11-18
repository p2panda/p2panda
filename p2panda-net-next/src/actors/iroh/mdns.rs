// SPDX-License-Identifier: MIT OR Apache-2.0

use std::num::ParseIntError;
use std::pin::Pin;
use std::str::FromStr;

use futures_util::{Stream, StreamExt};
use iroh::discovery::UserData;
use iroh::discovery::mdns::{DiscoveryEvent, MdnsDiscovery};
use iroh::endpoint_info::MaxLengthExceededError;
use p2panda_core::{IdentityError, Signature};
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use thiserror::Error;

use crate::TransportInfo;
use crate::actors::iroh::ToIrohEndpoint;
use crate::config::MdnsDiscoveryMode;

pub const MDNS_DISCOVERY: &str = "net.iroh.mdns";

pub enum ToMdns {
    Initialise(iroh::Endpoint, MdnsDiscoveryMode),
    NextStreamEvent,
}

pub type MdnsArguments = (iroh::Endpoint, MdnsDiscoveryMode, ActorRef<ToIrohEndpoint>);

pub struct MdnsState {
    iroh_endpoint_ref: ActorRef<ToIrohEndpoint>,
    stream: Option<Pin<Box<dyn Stream<Item = DiscoveryEvent>>>>,
}

#[derive(Default)]
pub struct Mdns;

impl ThreadLocalActor for Mdns {
    type Msg = ToMdns;

    type State = MdnsState;

    type Arguments = MdnsArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (endpoint, mode, iroh_endpoint_ref) = args;

        // Automatically initialise mDNS service after starting actor.
        myself.send_message(ToMdns::Initialise(endpoint, mode))?;

        Ok(MdnsState {
            iroh_endpoint_ref,
            stream: None,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToMdns::Initialise(endpoint, mode) => {
                if !mode.is_active() {
                    return Ok(());
                }

                let mdns = MdnsDiscovery::builder()
                    // Do not advertise our own endpoint address if in "passive" mode.
                    .advertise(mode.is_active())
                    .build(endpoint.id())?;

                // Make iroh endpoint aware of mDNS discovery service.
                endpoint.discovery().add(mdns.clone());

                // Start polling async stream of incoming discovery events.
                state.stream = Some(Box::pin(mdns.subscribe().await));
                myself.send_message(ToMdns::NextStreamEvent)?;
            }
            ToMdns::NextStreamEvent => {
                let Some(ref mut stream) = state.stream else {
                    unreachable!("tried to poll from mdns stream before initialising");
                };

                match stream.next().await {
                    Some(DiscoveryEvent::Discovered { endpoint_info, .. }) => {
                        let _ = state.iroh_endpoint_ref.send_message(
                            ToIrohEndpoint::UpdatedEndpointAddr {
                                endpoint_id: endpoint_info.endpoint_id,
                                user_data: endpoint_info.user_data().cloned(),
                                endpoint_addr: Some(endpoint_info.into()),
                            },
                        );
                    }
                    Some(DiscoveryEvent::Expired { .. }) => {
                        // At this point we know another node has not responded anymore within the
                        // local network.
                        //
                        // We can't do much here though since "removing" the iroh endpoint address
                        // from the transport info would need to be signed, and we don't have a
                        // signature here anymore.
                        //
                        // Additionally we don't know if that node might actually still be
                        // reachable (just not inside the same local area network).
                    }
                    None => {
                        // The stream has seized, close actor.
                        myself.stop(Some("mdns stream stopped".into()));
                    }
                }
            }
        }
        Ok(())
    }
}

const INFO_SEPARATOR: char = '.';

/// Helper to bring additional transport info (signature and timestamp) into a TXT DNS record.
///
/// We need this data to check the authenticity of the transport info.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportInfoTxt {
    signature: Signature,
    timestamp: u64,
}

impl TransportInfoTxt {
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
        UserData::try_from(TransportInfoTxt::from_transport_info(info))
    }
}

impl TryFrom<TransportInfoTxt> for UserData {
    type Error = MaxLengthExceededError;

    fn try_from(info: TransportInfoTxt) -> Result<Self, Self::Error> {
        // Encode the signature as an hex-string (128 characters) and the timestamp as a plain
        // number. There's a 245 character limit for user data.
        //
        // NOTE: This will currently fail if the u64 integer gets too large .. we can't "remote
        // crash" nodes because of that at least.
        UserData::try_from(format!(
            "{}{INFO_SEPARATOR}{}",
            info.signature, info.timestamp
        ))
    }
}

impl TryFrom<UserData> for TransportInfoTxt {
    type Error = TransportInfoTxtError;

    fn try_from(user_data: UserData) -> Result<Self, Self::Error> {
        let user_data = user_data.to_string();

        // Try to split string by separator into two halfs.
        let parts: Vec<_> = user_data.split(INFO_SEPARATOR).collect();
        if parts.len() != 2 {
            return Err(TransportInfoTxtError::InvalidSize(parts.len()));
        }

        // Try to parse halfs into signature and timestamp.
        let signature = Signature::from_str(parts.first().expect("we've checked the size before"))?;
        let timestamp = u64::from_str(parts.get(1).expect("we've checked the size before"))?;

        Ok(Self {
            signature,
            timestamp,
        })
    }
}

#[derive(Debug, Error)]
pub enum TransportInfoTxtError {
    #[error("invalid size of separated info parts, expected 2, given: {0}")]
    InvalidSize(usize),

    #[error(transparent)]
    InvalidSignature(#[from] IdentityError),

    #[error(transparent)]
    InvalidTimestamp(#[from] ParseIntError),
}

#[cfg(test)]
mod tests {
    use iroh::discovery::UserData;
    use p2panda_core::PrivateKey;

    use crate::TransportInfo;
    use crate::actors::iroh::mdns::TransportInfoTxt;

    #[test]
    fn transport_info_to_dns_txt() {
        // Create simple transport info object without any addresses attached.
        let private_key = PrivateKey::new();
        let transport_info = TransportInfo::new_unsigned().sign(&private_key).unwrap();

        // Extract information we want for our TXT record.
        let txt_info = TransportInfoTxt::from_transport_info(transport_info);

        // Convert it into iroh data type.
        let user_data = UserData::try_from(txt_info.clone()).unwrap();

        // .. and back!
        let txt_info_again = TransportInfoTxt::try_from(user_data).unwrap();
        assert_eq!(txt_info, txt_info_again);
    }
}
