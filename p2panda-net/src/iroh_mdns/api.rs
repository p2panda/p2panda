// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::ActorRef;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::address_book::AddressBook;
use crate::iroh_endpoint::Endpoint;
use crate::iroh_mdns::Builder;
use crate::iroh_mdns::actor::{MdnsActorArgs, ToMdns};

/// Resolve transport information for nearby nodes on the local-area network via multicast DNS
/// (mDNS).
///
/// ## Example
///
/// ```rust
/// # use std::error::Error;
/// #
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// # use futures_util::StreamExt;
/// # use p2panda_core::Hash;
/// # use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
/// # use p2panda_net::{AddressBook, Discovery, Endpoint, MdnsDiscovery, Gossip};
/// # let address_book = AddressBook::builder().spawn().await?;
/// # let endpoint = Endpoint::builder(address_book.clone())
/// #     .spawn()
/// #     .await?;
/// #
/// let mdns = MdnsDiscovery::builder(address_book, endpoint)
///     .mode(MdnsDiscoveryMode::Active)
///     .spawn()
///     .await?;
/// #
/// # Ok(())
/// # }
/// ```
///
/// ## Active vs. Passive Mode
///
/// mDNS Discovery is set to "passive" mode by default which allows you to resolve transport
/// information for other nodes without leaking your own information. Set it to "active" mode using
/// [`MdnsDiscoveryMode`](crate::iroh_mdns::MdnsDiscoveryMode) if you want to publish your IP
/// address on the local-area network.
#[derive(Clone)]
pub struct MdnsDiscovery {
    #[allow(unused)]
    pub(super) args: MdnsActorArgs,
    #[allow(unused)]
    pub(super) inner: Arc<RwLock<Inner>>,
}

pub(super) struct Inner {
    pub(super) actor_ref: Option<ActorRef<ToMdns>>,
}

impl MdnsDiscovery {
    pub(crate) fn new(actor_ref: Option<ActorRef<ToMdns>>, args: MdnsActorArgs) -> Self {
        Self {
            args,
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
        }
    }

    pub fn builder(address_book: AddressBook, endpoint: Endpoint) -> Builder {
        Builder::new(address_book, endpoint)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(actor_ref) = self.actor_ref.take() {
            actor_ref.stop(None);
        }
    }
}

#[derive(Debug, Error)]
pub enum MdnsDiscoveryError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Spawning the internal actor as a child actor of a supervisor failed.
    #[cfg(feature = "supervisor")]
    #[error(transparent)]
    ActorLinkedSpawn(#[from] crate::supervisor::SupervisorError),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToMdns>>),
}
