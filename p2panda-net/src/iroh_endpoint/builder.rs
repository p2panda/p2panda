// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use iroh::endpoint::EndpointHooks;
use p2panda_core::SigningKey;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

use crate::address_book::AddressBook;
use crate::iroh_endpoint::actors::{IrohEndpoint, IrohEndpointArgs};
use crate::iroh_endpoint::api::{Endpoint, EndpointError};
use crate::iroh_endpoint::config::IrohConfig;
use crate::iroh_endpoint::hooks::EndpointHooksList;
use crate::{DEFAULT_NETWORK_ID, NetworkId};

pub struct Builder {
    network_id: Option<NetworkId>,
    signing_key: Option<SigningKey>,
    config: Option<IrohConfig>,
    relays: HashMap<iroh::RelayUrl, Option<String>>,
    address_book: AddressBook,
    hooks: EndpointHooksList,
}

impl Builder {
    pub fn new(address_book: AddressBook) -> Self {
        Self {
            network_id: None,
            signing_key: None,
            config: None,
            relays: HashMap::new(),
            address_book,
            hooks: EndpointHooksList::new(),
        }
    }

    pub fn network_id(mut self, network_id: NetworkId) -> Self {
        self.network_id = Some(network_id);
        self
    }

    pub fn signing_key(mut self, signing_key: SigningKey) -> Self {
        self.signing_key = Some(signing_key);
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
        self.relays.insert(url, None);
        self
    }

    pub fn relay_url_with_token(mut self, url: iroh::RelayUrl, token: impl Into<String>) -> Self {
        self.relays.insert(url, Some(token.into()));
        self
    }

    /// Register custom hooks with the endpoint.
    ///
    /// Endpoint hooks intercept the connection establishment process of an iroh `Endpoint`.
    ///
    /// Multiple hooks can be registered with the `Endpoint` and will be called in their order of
    /// registration. If any of the hooks result in a connection rejection, all further processing
    /// is aborted and subsequent hooks will not be called.
    ///
    /// See iroh's [`EndpointHooks`] documentation for further details.
    ///
    /// [`EndpointHooks`]: https://docs.rs/iroh/latest/iroh/endpoint/trait.EndpointHooks.html
    pub fn hooks(mut self, hook: impl EndpointHooks + 'static + Clone) -> Self {
        self.hooks.push(hook);
        self
    }

    pub(crate) fn build_args(self) -> IrohEndpointArgs {
        let network_id = self.network_id.unwrap_or(DEFAULT_NETWORK_ID);
        let signing_key = self.signing_key.unwrap_or_default();
        let config = self.config.unwrap_or_default();

        let relay_map = self
            .relays
            .into_iter()
            .map(|(url, token)| match token {
                Some(token) => iroh::RelayConfig::from(url).with_auth_token(token),
                None => iroh::RelayConfig::from(url),
            })
            .collect();

        (
            network_id,
            signing_key,
            config,
            relay_map,
            self.address_book,
            self.hooks,
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
