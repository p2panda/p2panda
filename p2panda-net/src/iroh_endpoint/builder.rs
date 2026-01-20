// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;

use p2panda_core::PrivateKey;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

use crate::address_book::AddressBook;
use crate::iroh_endpoint::actors::{IrohEndpoint, IrohEndpointArgs};
use crate::iroh_endpoint::api::{Endpoint, EndpointError};
use crate::iroh_endpoint::config::IrohConfig;
use crate::{DEFAULT_NETWORK_ID, NetworkId};

pub struct Builder {
    network_id: Option<NetworkId>,
    private_key: Option<PrivateKey>,
    config: Option<IrohConfig>,
    relay_urls: HashSet<iroh::RelayUrl>,
    address_book: AddressBook,
}

impl Builder {
    pub fn new(address_book: AddressBook) -> Self {
        Self {
            network_id: None,
            private_key: None,
            config: None,
            address_book,
            relay_urls: HashSet::new(),
        }
    }

    pub fn network_id(mut self, network_id: NetworkId) -> Self {
        self.network_id = Some(network_id);
        self
    }

    pub fn private_key(mut self, private_key: PrivateKey) -> Self {
        self.private_key = Some(private_key);
        self
    }

    pub fn config(mut self, config: IrohConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Adds iroh relay server to assist in establishing direct connections.
    ///
    /// Multiple relays can be added, iroh will automatically choose a "home relay" based on
    /// latency.
    ///
    /// Relays fullfil multiple functions:
    ///
    /// 1. The relay server helps establish connections by temporarily routing encrypted traffic
    ///    until a direct, P2P connection is feasible. This allows nodes to immediately get
    ///    started, without waiting for holepunching / STUN to complete first.
    /// 2. Handle learning a node's public addresses (via QUIC address discovery), signalling and
    ///    hole-punching to establish direct connections between two nodes. This set of methods is
    ///    also understood as STUN. After this point the relay is not required anymore.
    /// 3. Relayed and encrypted fallback using the server when establishing a direct connection
    ///    failed (TURN).
    ///
    /// If no relay is given other nodes can only connect to us if a directly reachable IP address
    /// is available and known to them.
    pub fn relay_url(mut self, url: iroh::RelayUrl) -> Self {
        self.relay_urls.insert(url);
        self
    }

    pub(crate) fn build_args(self) -> IrohEndpointArgs {
        let network_id = self.network_id.unwrap_or(DEFAULT_NETWORK_ID);
        let private_key = self.private_key.unwrap_or_default();
        let config = self.config.unwrap_or_default();
        let relay_map = iroh::RelayMap::from_iter(self.relay_urls);
        (
            network_id,
            private_key,
            config,
            relay_map,
            self.address_book,
        )
    }

    pub async fn spawn(self) -> Result<Endpoint, EndpointError> {
        let args = self.build_args();

        let (actor_ref, _) = {
            let thread_pool = ThreadLocalActorSpawner::new();
            IrohEndpoint::spawn(None, args.clone(), thread_pool).await?
        };

        Ok(Endpoint::new(Some(actor_ref), args))
    }
}
