// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashSet};
use std::fmt::Debug;
use std::marker::PhantomData;

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::address_book::{AddressBookStore, NodeInfo};
use crate::traits::{DiscoveryProtocol, DiscoveryResult, LocalTopics};

///
///
use std::io::Write;

use blake3;
use rand::{RngCore, rng};
use thiserror;

const ALICE_SALT_BIT: [u8; 1] = [0];
const BOB_SALT_BIT: [u8; 1] = [1];

////
///

#[derive(Serialize, Deserialize)]
pub enum PsiHashDiscoveryMessage<ID, N>
where
    N: NodeInfo<ID>,
    for<'a> N::Transports: Serialize + Deserialize<'a>,
    ID: Ord,
{
    AliceSecretHalf {
        alice_salt_half: Vec<u8>,
    },
    BobSecretHalfAndHashedData {
        bob_salt_half: Vec<u8>,
        sync_topics_for_alice: HashSet<[u8; 32]>,
        ephemeral_messaging_topics_for_alice: HashSet<[u8; 32]>,
    },
    AliceHashedData {
        sync_topics_for_bob: HashSet<[u8; 32]>,
        ephemeral_messaging_topics_for_bob: HashSet<[u8; 32]>,
    },
    Nodes {
        transport_infos: BTreeMap<ID, N::Transports>,
    },
}

pub struct PsiHashDiscoveryProtocol<S, P, ID, N> {
    store: S,
    subscription: P,
    remote_node_id: ID,
    _marker: PhantomData<N>,
}

impl<S, P, ID, N> PsiHashDiscoveryProtocol<S, P, ID, N> {
    pub fn new(store: S, subscription: P, remote_node_id: ID) -> Self {
        Self {
            store,
            subscription,
            remote_node_id,
            _marker: PhantomData,
        }
    }
}

impl<S, P, ID, N> DiscoveryProtocol<ID, N> for PsiHashDiscoveryProtocol<S, P, ID, N>
where
    S: AddressBookStore<ID, N>,
    P: LocalTopics,
    ID: Clone + Ord,
    N: NodeInfo<ID>,
    for<'a> N::Transports: Serialize + Deserialize<'a>,
{
    type Error = PsiHashDiscoveryError<S, P, ID, N>;

    type Message = PsiHashDiscoveryMessage<ID, N>;

    async fn alice(
        &self,
        tx: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        rx: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> Result<DiscoveryResult<ID, N>, Self::Error> {
        let alice_salt_half = generate_salt();

        let message_1 = PsiHashDiscoveryMessage::AliceSecretHalf {
            alice_salt_half: alice_salt_half.clone(),
        };
        tx.send(message_1)
            .await
            .map_err(|_| PsiHashDiscoveryError::Sink)?;

        println!("alice wait m2");
        let message_2 = rx.next().await;
        let message_2 = match message_2 {
            Some(val) => val.map_err(|_| PsiHashDiscoveryError::Stream)?,
            None => {
                return Err(PsiHashDiscoveryError::Stream);
            }
        };

        let PsiHashDiscoveryMessage::BobSecretHalfAndHashedData {
            bob_salt_half,
            sync_topics_for_alice,
            ephemeral_messaging_topics_for_alice,
        } = message_2
        else {
            return Err(PsiHashDiscoveryError::UnexpectedMessage);
        };
        println!("alice recieve m2");
        // TODO: santity check bob salt half and bail if its 0s or short

        // now that bob has responded, lets grab our topics so we can hash them
        // keep topics as a vector so we can do lookups later
        let my_sync_topics: Vec<[u8; 32]> = self
            .subscription
            .sync_topics()
            .await
            .map_err(PsiHashDiscoveryError::Subscription)?
            .into_iter()
            .collect();
        println!("alice has {} sync topics", my_sync_topics.len());

        let my_ephemeral_topics: Vec<[u8; 32]> = self
            .subscription
            .ephemeral_messaging_topics()
            .await
            .map_err(PsiHashDiscoveryError::Subscription)?
            .into_iter()
            .collect();

        // final salts
        let alice_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &ALICE_SALT_BIT);
        let bob_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &BOB_SALT_BIT);

        let local_bob_sync_hashed: Vec<[u8; 32]> = hash_vector(&my_sync_topics, &bob_final_salt)?;

        //  my_sync_topics
        //     .iter()
        //     .cloned()
        //     .map(|topic| hash(&topic, &bob_final_salt))
        //     .collect::<Result<Vec<blake3::Hash>, _>>()?
        //     .into_iter()
        //     .map(|h| h.as_bytes().clone())
        //     .collect();

        let bob_hashed_for_alice_set: HashSet<[u8; 32]> =
            HashSet::from_iter(sync_topics_for_alice.iter().cloned());

        // alice computes intersection of her own work with what bob sent her
        println!("alice hashed items {:?} ", local_bob_hashed);
        println!("alice from bob {:?}", bob_hashed_for_alice_set);

        let mut alice_intersection: HashSet<[u8; 32]> = HashSet::new();
        for (i, local_hash) in local_bob_hashed.iter().enumerate() {
            if bob_hashed_for_alice_set.contains(local_hash) {
                alice_intersection.insert(my_sync_topics[i].clone());
            }
        }

        // now alice needs to hash her data with her salt and send to bob so he can do the same

        let alice_hashed_for_bob: HashSet<[u8; 32]> = HashSet::from_iter(
            my_sync_topics
                .into_iter()
                .map(|topic| hash(&topic, &alice_final_salt))
                .collect::<Result<Vec<blake3::Hash>, _>>()?
                .into_iter()
                .map(|h| h.as_bytes().clone()),
        );

        println!("alice send final");

        tx.send(PsiHashDiscoveryMessage::AliceHashedData {
            sync_topics_for_bob: alice_hashed_for_bob,
            ephemeral_messaging_topics_for_bob: HashSet::new(), // todo for real
        })
        .await
        .map_err(|_| PsiHashDiscoveryError::Sink)?;

        println!("alice await recieve message 4 from bob ");
        // todo return real data

        let message_4 = rx.next().await;
        let message_4 = match message_4 {
            Some(val) => val.map_err(|_| PsiHashDiscoveryError::Stream)?,
            None => {
                return Err(PsiHashDiscoveryError::Stream);
            }
        };

        println!("alice recieve message4, unparsed");
        let PsiHashDiscoveryMessage::Nodes { transport_infos } = message_4 else {
            return Err(PsiHashDiscoveryError::UnexpectedMessage);
        };

        let node_infos = self
            .store
            .all_node_infos()
            .await
            .map_err(PsiHashDiscoveryError::Store)?;

        println!("alice send message 5");
        tx.send(PsiHashDiscoveryMessage::Nodes {
            transport_infos: {
                let mut map = BTreeMap::new();
                for node_info in node_infos {
                    if let Some(transport_info) = node_info.transports() {
                        map.insert(node_info.id(), transport_info);
                    }
                }
                map
            },
        })
        .await
        .map_err(|_| PsiHashDiscoveryError::Sink)?;

        println!("alice done, OK!");

        Ok(DiscoveryResult {
            remote_node_id: self.remote_node_id.clone(),
            node_transport_infos: transport_infos,
            sync_topics: alice_intersection,
            ephemeral_messaging_topics: HashSet::new(),
        })
    }

    async fn bob(
        &self,
        tx: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        rx: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> Result<DiscoveryResult<ID, N>, Self::Error> {
        let Some(Ok(message_1)) = rx.next().await else {
            return Err(PsiHashDiscoveryError::Stream);
        };

        println!("bob recieve message 1");

        let PsiHashDiscoveryMessage::AliceSecretHalf { alice_salt_half } = message_1 else {
            return Err(PsiHashDiscoveryError::UnexpectedMessage);
        };

        let bob_salt_half = generate_salt();

        // final salts
        let alice_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &ALICE_SALT_BIT);
        let bob_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &BOB_SALT_BIT);

        let my_sync_topics: Vec<[u8; 32]> = self
            .subscription
            .sync_topics()
            .await
            .map_err(PsiHashDiscoveryError::Subscription)?
            .into_iter()
            .collect();

        let my_ephemeral_topics: Vec<[u8; 32]> = self
            .subscription
            .ephemeral_messaging_topics()
            .await
            .map_err(PsiHashDiscoveryError::Subscription)?
            .into_iter()
            .collect();

        let sync_topics_for_alice: HashSet<[u8; 32]> = HashSet::from_iter(
            my_sync_topics
                .iter()
                .cloned()
                .map(|topic| hash(&topic, &bob_final_salt))
                .collect::<Result<Vec<blake3::Hash>, _>>()?
                .into_iter()
                .map(|h| h.as_bytes().clone()),
        );

        let ephemeral_topics_for_alice: HashSet<[u8; 32]> = HashSet::from_iter(
            my_sync_topics
                .iter()
                .cloned()
                .map(|topic| hash(&topic, &bob_final_salt))
                .collect::<Result<Vec<blake3::Hash>, _>>()?
                .into_iter()
                .map(|h| h.as_bytes().clone()),
        );

        println!("bob send message 2");
        tx.send(PsiHashDiscoveryMessage::BobSecretHalfAndHashedData {
            bob_salt_half,
            sync_topics_for_alice,
            ephemeral_messaging_topics_for_alice: ephemeral_topics_for_alice,
        })
        .await
        .map_err(|_| PsiHashDiscoveryError::Sink)?;

        println!("bob wait for message 3");
        let Some(Ok(message_3)) = rx.next().await else {
            return Err(PsiHashDiscoveryError::Stream);
        };

        let PsiHashDiscoveryMessage::AliceHashedData {
            sync_topics_for_bob,
            ephemeral_messaging_topics_for_bob,
        } = message_3
        else {
            return Err(PsiHashDiscoveryError::UnexpectedMessage);
        };

        println!("bob recieved message 3");

        let sync_topics_intersection =
            compute_intersection(&my_sync_topics, &sync_topics_for_bob, &alice_final_salt)?;

        
        let ephemral_topics_intersection =
            compute_intersection(&my_ephemeral_topics, &ephemeral_messaging_topics_for_bob, &alice_final_salt)?;

        // send alice our nodes we know about
        let node_infos = self
            .store
            .all_node_infos()
            .await
            .map_err(PsiHashDiscoveryError::Store)?;

        println!("bob try send message 4, Nodes");
        tx.send(PsiHashDiscoveryMessage::Nodes {
            transport_infos: {
                let mut map = BTreeMap::new();
                for node_info in node_infos {
                    if let Some(transport_info) = node_info.transports() {
                        map.insert(node_info.id(), transport_info);
                    }
                }
                map
            },
        })
        .await
        .map_err(|_| PsiHashDiscoveryError::Sink)?;

        println!("bob wait for message 5, nodes");
        let Some(Ok(message_5)) = rx.next().await else {
            return Err(PsiHashDiscoveryError::Stream);
        };

        let PsiHashDiscoveryMessage::Nodes { transport_infos } = message_5 else {
            return Err(PsiHashDiscoveryError::UnexpectedMessage);
        };

        println!("bob done, Ok!");

        Ok(DiscoveryResult {
            remote_node_id: self.remote_node_id.clone(),
            node_transport_infos: transport_infos,
            sync_topics: sync_topics_intersection,
            ephemeral_messaging_topics: ephemral_topics_intersection
        })
    }
}

pub fn compute_intersection<S, P, ID, N>(
    local_topics: &Vec<[u8; 32]>,
    remote_hashes: &HashSet<[u8; 32]>,
    salt: &Vec<u8>,
) -> Result<HashSet<[u8; 32]>, PsiHashDiscoveryError<S, P, ID, N>>
where
    S: AddressBookStore<ID, N>,
    P: LocalTopics,
    ID: Clone + Ord,
    N: NodeInfo<ID>,
    for<'a> N::Transports: Serialize + Deserialize<'a>,
{
    let alice_hashed_for_bob_map: HashSet<[u8; 32]> =
        HashSet::from_iter(remote_hashes.iter().cloned());

    let local_alice_hashed: Vec<[u8; 32]> = local_topics
        .iter()
        .cloned()
        .map(|topic| hash(&topic, &salt))
        .collect::<Result<Vec<blake3::Hash>, _>>()?
        .iter()
        .map(|h| h.as_bytes().clone())
        .collect();

    // bob computes intersection
    let mut bob_intersection: HashSet<[u8; 32]> = HashSet::new();
    for (i, local_hash) in local_alice_hashed.iter().enumerate() {
        if alice_hashed_for_bob_map.contains(local_hash) {
            bob_intersection.insert(local_topics[i].clone());
        }
    }

    Ok(bob_intersection)
}

fn hash_vector(topics: &Vec<[u8;32]>, salt: &[u8]) -> Result<Vec<blake3::Hash>, std::io::Error> {
    topics
        .iter()
        .map(|topic| hash(&topic, salt))
        .collect()
}

pub fn hash(
    data: &[u8; 32],
    salt: &[u8], // TODO make this a fixed or minimum length?
) -> Result<blake3::Hash, std::io::Error>
{
    let mut hash = blake3::Hasher::new();
    hash.write_all(data)?;
    hash.write_all(salt)?;
    Ok(hash.finalize())
}

pub fn generate_salt() -> Vec<u8> {
    let mut generator = rng();

    let mut random_bytes: Vec<u8> = vec![0; 32];
    generator.fill_bytes(&mut random_bytes);

    random_bytes
}

pub fn combine_salt(
    alice_salt_half: &Vec<u8>,
    bob_salt_half: &Vec<u8>,
    pair_byte: &[u8; 1],
) -> Vec<u8> {
    // let concat_b64 = vec![alice_b64, bob_b64];
    let mut output = alice_salt_half.to_owned();
    output.extend_from_slice(alice_salt_half);
    output.extend_from_slice(&bob_salt_half);
    output.extend_from_slice(pair_byte);
    output
}

#[derive(Debug, Error)]
pub enum PsiHashDiscoveryError<S, P, ID, N>
where
    S: AddressBookStore<ID, N>,
    P: LocalTopics,
{
    #[error("{0}")]
    Store(S::Error),

    #[error("{0}")]
    Subscription(P::Error),

    #[error("received unexpected message")]
    UnexpectedMessage,

    #[error("stream closed unexpectedly")]
    Stream,

    #[error("sink closed unexpectedly")]
    Sink,

    #[error(transparent)]
    Hash(#[from] std::io::Error)
}