// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(doctest, doc=include_str!("../README.md"))]

//! Peer discovery traits and services.
//!
//! This crate currently provides a single discovery service implementation: mDNS. It is disabled
//! by default and can be selected by enabling the `mdns` feature flag.
//!
//! Generic traits are provided to facitilate the creation of other peer discovery implementations.
#[cfg(feature = "mdns")]
pub mod mdns;

use std::fmt::Debug;
use std::pin::Pin;

use anyhow::Result;
use futures_buffered::MergeBounded;
use futures_lite::stream::Stream;
use iroh::NodeAddr;

pub type BoxedStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

/// A collection of discovery services.
///
/// `DiscoveryMap` implements the `Discovery` trait to provide a convenient means of subscribing to
/// a single stream comprising all events from multiple discovery strategies. This also allows updating the address
/// information of the local node for all discovery services with a single call to
/// `update_local_address`.
#[derive(Debug, Default)]
pub struct DiscoveryMap {
    services: Vec<Box<dyn Discovery>>,
}

impl DiscoveryMap {
    /// Instantiate a `DiscoveryMap` from a list of services.
    pub fn from_services(services: Vec<Box<dyn Discovery>>) -> Self {
        Self { services }
    }

    /// Add a single discovery service to the map.
    pub fn add(&mut self, service: impl Discovery + 'static) {
        self.services.push(Box::new(service));
    }
}

impl Discovery for DiscoveryMap {
    fn subscribe(&self, network_id: [u8; 32]) -> Option<BoxedStream<Result<DiscoveryEvent>>> {
        let streams = self
            .services
            .iter()
            .filter_map(|service| service.subscribe(network_id));
        let streams = MergeBounded::from_iter(streams);
        Some(Box::pin(streams))
    }

    fn update_local_address(&self, addr: &NodeAddr) -> Result<()> {
        for service in &self.services {
            service.update_local_address(addr)?;
        }
        Ok(())
    }
}

/// An event emitted when a peer is discovered.
///
/// Includes the addressing information of the peer, along with the identifier of the service
/// through which the peer was discovered.
#[derive(Debug, Clone)]
pub struct DiscoveryEvent {
    /// Identifier of the discovery service from which this event originated from.
    pub provenance: &'static str,

    /// Addressing information of a discovered peer.
    pub node_addr: NodeAddr,
}

/// An interface for announcing and discovering network peers.
///
/// The `Discovery` trait provides a generic interface for discovering the identities and
/// addressing information of peers on a network, as well as sharing that same information for the
/// local node.
///
/// The discovery process facilitates network connectivity for more robust communication. It can
/// serve as a network bootstrapping mechanism, in the case of mDNS, or as a means of expanding
/// network knowledge after initial entry (for example, via a rendezvous server).
pub trait Discovery: Debug + Send + Sync {
    /// Update the addressing information for the local node.
    fn update_local_address(&self, node_addr: &NodeAddr) -> Result<()>;

    /// Subscribe to a stream of discovery events for the given network.
    fn subscribe(&self, _network_id: [u8; 32]) -> Option<BoxedStream<Result<DiscoveryEvent>>> {
        None
    }
}
