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
    /// Returns information about a node.
    ///
    /// Returns `None` if no information was found for this node.
    NodeInfo(NodeId, RpcReplyPort<Option<NodeInfo>>),

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
            ToAddressBook::NodeInfo(node_id, reply) => {
                let result = state.store.node_info(&node_id).await?;
                let _ = reply.send(result);
            }
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
                // (it's a simple "last write wins" principle based on a UNIX timestamp) handled
                // inside of `update_transports`.
                //
                // If a node info already exists, only update the "transports" aspect of it and
                // keep any other "local" configuration, otherwise create a new "default" node info.
                let current = state.store.node_info(&node_id).await?;
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

#[cfg(test)]
mod tests {
    use p2panda_core::PrivateKey;
    use p2panda_discovery::address_book::memory::MemoryStore;
    use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
    use ractor::{Actor, call};
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    use crate::actors::{generate_actor_namespace, with_namespace};
    use crate::addrs::{NodeId, NodeInfo};
    use crate::args::ApplicationArguments;
    use crate::{TransportAddress, UnsignedTransportInfo};

    use super::{ADDRESS_BOOK, AddressBook, ToAddressBook};

    type TestTopic = ();

    #[tokio::test]
    async fn insert_node_and_transport_info() {
        let private_key = PrivateKey::new();
        let public_key = private_key.public_key();
        let actor_namespace = generate_actor_namespace(&public_key);
        let spawner = ThreadLocalActorSpawner::new();
        let rng = ChaCha20Rng::from_seed([1; 32]);

        let (actor, handle) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &actor_namespace)),
            ApplicationArguments {
                private_key: private_key.clone(),
                store: MemoryStore::<ChaCha20Rng, TestTopic, NodeId, NodeInfo>::new(rng),
            },
            spawner,
        )
        .await
        .unwrap();

        // Insert new node info.
        let node_info = NodeInfo::new(public_key);
        let result = call!(actor, ToAddressBook::InsertNodeInfo, node_info).unwrap();
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Overwriting node info should return "false".
        let mut node_info = NodeInfo::new(public_key);
        node_info.bootstrap = true;
        let result = call!(actor, ToAddressBook::InsertNodeInfo, node_info).unwrap();
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Bootstrap should be set to "true", as node info was still overwritten.
        let result = call!(actor, ToAddressBook::NodeInfo, public_key).unwrap();
        assert!(result.expect("node info exists in store").bootstrap);

        // Inserting invalid node info should fail.
        let node_info = {
            NodeInfo {
                node_id: public_key,
                bootstrap: false,
                transports: Some({
                    let mut unsigned = UnsignedTransportInfo::new();
                    unsigned.add_addr(TransportAddress::from_iroh(
                        public_key,
                        Some("https://my.relay.net".parse().unwrap()),
                        [],
                    ));
                    let mut transport_info = unsigned.sign(&private_key).unwrap();
                    transport_info.timestamp = 1234; // Manipulate timestamp to make signature invalid
                    transport_info
                }),
            }
        };
        assert!(node_info.verify().is_err());
        let result = call!(actor, ToAddressBook::InsertNodeInfo, node_info).unwrap();
        assert!(result.is_err());

        // Inserting transport info should not overwrite "local" data.
        let mut node_info = NodeInfo::new(public_key);
        node_info.bootstrap = true;
        let result = call!(actor, ToAddressBook::InsertNodeInfo, node_info).unwrap();
        assert!(result.is_ok());

        let transport_info = {
            let mut unsigned = UnsignedTransportInfo::new();
            unsigned.add_addr(TransportAddress::from_iroh(
                public_key,
                Some("https://my.relay.net".parse().unwrap()),
                [],
            ));
            unsigned.sign(&private_key).unwrap()
        };
        let result = call!(
            actor,
            ToAddressBook::InsertTransportInfo,
            public_key,
            transport_info
        )
        .unwrap();
        assert!(result.is_ok());

        // Even after insertion of new transport info, the "local" bootstrap config is still true.
        let result = call!(actor, ToAddressBook::NodeInfo, public_key).unwrap();
        assert!(result.expect("node info exists in store").bootstrap);

        // Inserting invalid transport info should fail.
        let transport_info = {
            let mut unsigned = UnsignedTransportInfo::new();
            unsigned.add_addr(TransportAddress::from_iroh(
                public_key,
                Some("https://my.relay.net".parse().unwrap()),
                [],
            ));
            let mut transport_info = unsigned.sign(&private_key).unwrap();
            transport_info.timestamp = 1234; // Manipulate timestamp to make signature invalid
            transport_info
        };
        assert!(transport_info.verify(&public_key).is_err());
        let result = call!(
            actor,
            ToAddressBook::InsertTransportInfo,
            public_key,
            transport_info
        )
        .unwrap();
        assert!(result.is_err());
    }
}
