// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashSet};
use std::fmt::Debug;
use std::marker::PhantomData;

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::address_book::{AddressBookStore, NodeInfo};
use crate::traits::{DiscoveryProtocol, DiscoveryResult, LocalTopics};

use std::io::Write;

use blake3;
use rand::{RngCore, rng};
use thiserror;

const ALICE_SALT_BIT: u8 = 0;
const BOB_SALT_BIT: u8 = 1;

#[derive(Serialize, Deserialize)]
pub enum PsiHashDiscoveryMessage<ID, N>
where
    N: NodeInfo<ID>,
    for<'a> N::Transports: Serialize + Deserialize<'a>,
    ID: Ord,
{
    AliceSecretHalf {
        alice_salt_half: [u8; 32],
    },
    BobSecretHalfAndHashedData {
        bob_salt_half: [u8; 32],
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

        let message_2 = match rx.next().await {
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

        // final salts
        let alice_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &ALICE_SALT_BIT);
        let bob_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &BOB_SALT_BIT);

        // // alice computes intersection of her own work with what bob sent her
        let sync_topics_intersection =
            compute_intersection(&my_sync_topics, &sync_topics_for_alice, &bob_final_salt)?;
        let ephemeral_messaging_topics_intersection = compute_intersection(
            &my_ephemeral_topics,
            &ephemeral_messaging_topics_for_alice,
            &bob_final_salt,
        )?;

        // now alice needs to hash her data with her salt and send to bob so he can do the same
        let sync_topics_for_bob: HashSet<[u8; 32]> =
            HashSet::from_iter(hash_vector(&my_sync_topics, &alice_final_salt)?.into_iter());
        let ephemeral_messaging_topics_for_bob: HashSet<[u8; 32]> =
            HashSet::from_iter(hash_vector(&my_ephemeral_topics, &alice_final_salt)?.into_iter());

        tx.send(PsiHashDiscoveryMessage::AliceHashedData {
            sync_topics_for_bob,
            ephemeral_messaging_topics_for_bob,
        })
        .await
        .map_err(|_| PsiHashDiscoveryError::Sink)?;

        let message_4 = match rx.next().await {
            Some(val) => val.map_err(|_| PsiHashDiscoveryError::Stream)?,
            None => {
                return Err(PsiHashDiscoveryError::Stream);
            }
        };

        let PsiHashDiscoveryMessage::Nodes { transport_infos } = message_4 else {
            return Err(PsiHashDiscoveryError::UnexpectedMessage);
        };

        let node_infos = self
            .store
            .all_node_infos()
            .await
            .map_err(PsiHashDiscoveryError::Store)?;

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

        Ok(DiscoveryResult {
            remote_node_id: self.remote_node_id.clone(),
            node_transport_infos: transport_infos,
            sync_topics: sync_topics_intersection,
            ephemeral_messaging_topics: ephemeral_messaging_topics_intersection,
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

        let PsiHashDiscoveryMessage::AliceSecretHalf { alice_salt_half } = message_1 else {
            return Err(PsiHashDiscoveryError::UnexpectedMessage);
        };
        let bob_salt_half = generate_salt();

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

        let sync_topics_for_alice: HashSet<[u8; 32]> =
            HashSet::from_iter(hash_vector(&my_sync_topics, &bob_final_salt)?.into_iter());
        let ephemeral_messaging_topics_for_alice: HashSet<[u8; 32]> =
            HashSet::from_iter(hash_vector(&my_ephemeral_topics, &bob_final_salt)?.into_iter());

        tx.send(PsiHashDiscoveryMessage::BobSecretHalfAndHashedData {
            bob_salt_half,
            sync_topics_for_alice,
            ephemeral_messaging_topics_for_alice,
        })
        .await
        .map_err(|_| PsiHashDiscoveryError::Sink)?;

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

        let sync_topics_intersection =
            compute_intersection(&my_sync_topics, &sync_topics_for_bob, &alice_final_salt)?;

        let ephemral_topics_intersection = compute_intersection(
            &my_ephemeral_topics,
            &ephemeral_messaging_topics_for_bob,
            &alice_final_salt,
        )?;

        // send alice our nodes we know about
        let node_infos = self
            .store
            .all_node_infos()
            .await
            .map_err(PsiHashDiscoveryError::Store)?;

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

        let Some(Ok(message_5)) = rx.next().await else {
            return Err(PsiHashDiscoveryError::Stream);
        };

        let PsiHashDiscoveryMessage::Nodes { transport_infos } = message_5 else {
            return Err(PsiHashDiscoveryError::UnexpectedMessage);
        };

        Ok(DiscoveryResult {
            remote_node_id: self.remote_node_id.clone(),
            node_transport_infos: transport_infos,
            sync_topics: sync_topics_intersection,
            ephemeral_messaging_topics: ephemral_topics_intersection,
        })
    }
}

pub fn compute_intersection(
    local_topics: &Vec<[u8; 32]>,
    remote_hashes: &HashSet<[u8; 32]>,
    salt: &[u8; 65],
) -> Result<HashSet<[u8; 32]>, std::io::Error> {
    let local_topics_hashed = hash_vector(local_topics, salt)?;

    let mut intersection: HashSet<[u8; 32]> = HashSet::new();
    for (i, local_hash) in local_topics_hashed.iter().enumerate() {
        if remote_hashes.contains(local_hash) {
            intersection.insert(local_topics[i].clone());
        }
    }
    Ok(intersection)
}

fn hash_vector(topics: &Vec<[u8; 32]>, salt: &[u8; 65]) -> Result<Vec<[u8; 32]>, std::io::Error> {
    topics.iter().map(|topic| hash(&topic, salt)).collect()
}

pub fn hash(data: &[u8; 32], salt: &[u8; 65]) -> Result<[u8; 32], std::io::Error> {
    let mut hash = blake3::Hasher::new();
    hash.write_all(data)?;
    hash.write_all(salt)?;
    Ok(hash.finalize().as_bytes().clone())
}

pub fn generate_salt() -> [u8; 32] {
    let mut generator = rng();
    let mut random_bytes: [u8; 32] = [0; 32];
    generator.fill_bytes(&mut random_bytes);
    random_bytes
}

pub fn combine_salt(
    alice_salt_half: &[u8; 32],
    bob_salt_half: &[u8; 32],
    pair_byte: &u8,
) -> [u8; 65] {
    let mut output: [u8; 65] = [0; 65];
    output[0..32].copy_from_slice(alice_salt_half);
    output[32..64].copy_from_slice(bob_salt_half);
    output[64] = *pair_byte;
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
    Hash(#[from] std::io::Error),
}
