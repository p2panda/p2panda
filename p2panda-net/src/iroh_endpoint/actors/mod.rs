// SPDX-License-Identifier: MIT OR Apache-2.0

mod connection;
mod endpoint;

pub use endpoint::{ConnectError, IrohEndpoint, IrohEndpointArgs, ToIrohEndpoint};

/// Returns true if endpoint is globally reachable.
pub(crate) fn is_globally_reachable_endpoint(addr: iroh::EndpointAddr) -> bool {
    addr.ip_addrs()
        .any(|addr| crate::utils::connectivity_status(addr).is_global())
}
