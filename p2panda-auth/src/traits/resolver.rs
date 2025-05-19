// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

/// Resolver trait used in op-based CRDT for producing operation filters when concurrent
/// operations cause conflicts which require special handling.
///
/// The generic parameter S is the state of the CRDT itself.
pub trait Resolver<S, MSG> {
    type Error: Error;

    // Check if this message requires that a full state re-build takes place. This would usually
    // be due to concurrent operations arriving which require special handling.
    fn rebuild_required(y: &S, msg: &MSG) -> bool;

    // Process all operations and update internal state as required.
    //
    // This could include updating any internal filter object.
    fn process(y: S) -> Result<S, Self::Error>;
}
