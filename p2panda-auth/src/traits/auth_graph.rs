use std::error::Error;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::traits::{Ordering, Resolver};

/// Interface for implementing an "auth graph".
///
/// Auth graph is an operation-based CRDT with a "prepare" method which takes an operation and
/// enriches it (based on local state) with meta-data required for processing locally, or
/// remotely. And a "process" method for processing operations created locally or by remote peers.  
///
/// Generic parameter RS (resolver) allows for introducing custom logic which decides if
/// operations should be included in any state-deriving process. This can include the handling of
/// concurrent operations which would cause conflicting state changes.
pub trait AuthGraph<ID, OP, RS, ORD>
where
    RS: Clone + Resolver<Self::State, ORD::Message>,
    ORD: Clone + Ordering<ID, OP, Self::Action>,
{
    type State: Clone + Debug + Serialize + for<'a> Deserialize<'a>;
    type Action;
    type Error: Error;

    /// Prepare an action for processing.
    ///
    /// Meta-data like author identity, signature, or local-time should be added in this method
    /// and an operation is returned which can be processed locally or sent to a remote peer.
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
