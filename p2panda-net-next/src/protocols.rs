// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;

use p2panda_core::Hash;

use crate::{NetworkId, NodeId};

/// Unique byte identifier for a network protocol.
///
/// The protocol identifier is supplied along with a protocol handler when registering a network
/// protocol.
///
/// A hash function is performed against each network protocol identifier which is registered with
/// `p2panda-net`. Even if two instances of `p2panda-net` are created with the same network
/// protocols, any communication attempts will fail if they are not using the same network
/// identifier.
pub type ProtocolId = Vec<u8>;

/// Hash the concatenation of the given protocol- and network identifiers.
pub fn hash_protocol_id_with_network_id(
    protocol_id: impl AsRef<[u8]>,
    network_id: &NetworkId,
) -> [u8; 32] {
    Hash::new([protocol_id.as_ref(), network_id].concat()).into()
}

pub trait ProtocolHandler: Send + Sync + 'static {
    // @TODO: What's the error type?
    //
    // From iroh's docs: "The returned future runs on a freshly spawned tokio task so it can be
    // long-running. Once `accept()` returns, the connection is closed, so the returned future is
    // expected to drive the protocol to completion.  If there in a protocol error, you can use
    // [`Connection::close`] to send an error code to the remote peer.  Returning an
    // `Err<AcceptError>` will also close the connection and log a warning, but no dedicated error
    // code will be sent to the peer, so it's recommended to explicitly close the connection within
    // your accept handler."
    fn accept(
        &self,
        remote_node_id: NodeId,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>;

    fn shutdown(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {})
    }
}
