// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(dead_code)]

use std::error::Error;

use crate::traits::{IdentityHandle, OperationId, Ordering};

/// Interface for implementing an auth group.
///
/// Auth group interface contains a "prepare" method which takes an operation and enriches it
/// (based on local state) with meta-data required for processing locally, or remotely. And a
/// "process" method for processing operations created locally or by remote peers.  
///
/// Generic parameter RS (resolver) allows for introducing custom logic which decides if
/// operations should be included in any state-deriving process. This can include the handling of
/// concurrent operations which would cause conflicting state changes.
pub trait AuthGroup<ID, OP, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    ORD: Ordering<ID, OP, Self::Action>,
{
    type State;
    type Action;
    type Error: Error;

    /// Prepare an action for processing.
    ///
    /// Meta-data like author identity, signature, or ordering clock should be added in this
    /// method and an operation is returned which can be processed locally or sent to a remote
    /// peer.
    fn prepare(
        y: Self::State,
        action: &Self::Action,
    ) -> Result<(Self::State, ORD::Message), Self::Error>;

    /// Process a prepared operation.
    ///
    /// Both locally created and operations received from the network should be processed with this
    /// method.
    fn process(y: Self::State, operation: &ORD::Message) -> Result<Self::State, Self::Error>;
}
