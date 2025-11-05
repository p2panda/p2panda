// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::error::Error as StdError;
use std::marker::PhantomData;

// @TODO: This will come from `p2panda-store` eventually.
use p2panda_discovery::address_book::AddressBookStore;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, Message, RpcReplyPort};
use thiserror::Error;

use crate::args::ApplicationArguments;
use crate::{NodeId, NodeInfo, TopicId, TransportInfo};

/// Address book actor name.
pub const ADDRESS_BOOK: &str = "net.address_book";

pub enum ToAddressBook {
    /// Inserts or updates node information into address book. Use this method if adding node
    /// information from a local configuration, trusted, external source, etc.
    ///
    /// Returns `true` if entry got newly inserted or `false` if existing entry was updated.
    /// Previous entries are simply overwritten. Entries with attached transport information get
    /// checked against authenticity and throw an error otherwise.
    InsertNodeInfo(NodeInfo, RpcReplyPort<Result<bool, AddressBookError>>),

    /// Inserts or updates attached transport info for a node. Use this method if adding transport
    /// information from an untrusted source.
    ///
    /// Transport information is usually exchanged as part of a discovery protocol and should be
    /// considered untrusted.
    ///
    /// This method checks if the given information is authentic and uses a timestamp to apply a
    /// "last write wins" rule. It retuns `true` if the given entry overwritten the previous one or
    /// `false` if the previous entry is already the latest.
    ///
    /// Local data of the node information stay untouched if they already exist, only the
    /// "transports" aspect gets inserted / updated.
    InsertTransportInfo(
        NodeId,
        TransportInfo,
        RpcReplyPort<Result<bool, AddressBookError>>,
    ),
}

pub struct AddressBookState<S> {
    store: S,
}

pub struct AddressBook<S, T> {
    _marker: PhantomData<(S, T)>,
}

impl<S, T> Default for AddressBook<S, T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S, T> ThreadLocalActor for AddressBook<S, T>
where
    S: AddressBookStore<T, NodeId, NodeInfo> + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
    T: 'static,
{
    type State = AddressBookState<S>;

    type Msg = ToAddressBook;

    // @TODO: For now we leave out the concept of a `NetworkId` but we may want some way to slice
    // address subsets in the future.
    type Arguments = ApplicationArguments<S>;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(AddressBookState { store: args.store })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Note that critical storage failures will return an `ActorProcessingErr` and cause this
        // actor to restart when supervised.
        match message {
            ToAddressBook::InsertNodeInfo(node_info, reply) => {
                // Check signature of information. Is it authentic?
                if let Err(err) = node_info
                    .verify()
                    .map_err(|err| AddressBookError::NodeInfo(err))
                {
                    let _ = reply.send(Err(err));
                    return Ok(());
                }

                // Overwrite any previously given information if it existed.
                let result = state.store.insert_node_info(node_info).await?;
                let _ = reply.send(Ok(result));
            }
            ToAddressBook::InsertTransportInfo(node_id, transport_info, reply) => {
                // Check signature of information. Is it authentic?
                if let Err(err) = transport_info
                    .verify(&node_id)
                    .map_err(|err| AddressBookError::NodeInfo(err))
                {
                    let _ = reply.send(Err(err));
                    return Ok(());
                }

                // Is there already an existing entry? Only replace it when information is newer
                // (it's a simple "last write wins" principle based on a UNIX timestamp).
                let current = state.store.node_info(&node_id).await?;
                let current_timestamp = {
                    let current_transports = current
                        .as_ref()
                        .map(|info| info.transports.as_ref())
                        .flatten();
                    current_transports.map(|transports| transports.timestamp)
                };

                let replace_transport_info = match current_timestamp {
                    Some(current_timestamp) => current_timestamp < transport_info.timestamp,
                    None => true,
                };

                if !replace_transport_info {
                    let _ = reply.send(Ok(false));
                    return Ok(());
                }

                // If a node info already exists, only update the "transports" aspect of it and
                // keep any other "local" configuration, otherwise create a new "default" node info.
                let node_info = match current {
                    Some(mut current) => current.update_transports(transport_info).map(|_| current),
                    None => Ok(NodeInfo::new(node_id)),
                };

                let result = match node_info {
                    Ok(node_info) => {
                        let result = state.store.insert_node_info(node_info).await?;
                        Ok(result)
                    }
                    Err(err) => Err(AddressBookError::NodeInfo(err)),
                };

                let _ = reply.send(result);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum AddressBookError {
    #[error(transparent)]
    NodeInfo(crate::addrs::NodeInfoError),
}
