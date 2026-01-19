// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashSet};
use std::fmt::Debug;
use std::io::Write;
use std::marker::PhantomData;

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use rand::{RngCore, rng};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::address_book::{AddressBookStore, NodeInfo};
use crate::traits::{DiscoveryProtocol, DiscoveryResult, LocalTopics};

const ALICE_SALT_BYTE: u8 = 0;
const BOB_SALT_BYTE: u8 = 1;

/// PSI protocol message.
///
/// Alice (the initiator) and Bob (the accepter) exchange these messages in the order they are
/// written here. Any out of order message recieved should result in the protocol returning an
/// error.
#[derive(Serialize, Deserialize)]
pub enum PsiHashMessage<ID, N>
where
    N: NodeInfo<ID>,
    for<'a> N::Transports: Serialize + Deserialize<'a>,
    ID: Ord,
{
    /// Alice initiates, sending bob her 32 bytes of randomness for half of the salt.
    AliceSaltHalf { alice_salt_half: [u8; 32] },

    /// Bob sends back his half of the salt, along with his topics hashed using the combined salt.
    BobSaltHalfAndHashedData {
        bob_salt_half: [u8; 32],
        topics_for_alice: HashSet<[u8; 32]>,
    },

    /// Alice replies with her own hashed topics.
    AliceHashedData { topics_for_bob: HashSet<[u8; 32]> },

    /// Both peers then also exchange a list of other nodes they are aware of for peer discovery.
    Nodes {
        transport_infos: BTreeMap<ID, N::Transports>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Config {
    /// If enabled the protocol will only share transport infos of nodes which have at least one
    /// common topic with the remote node.
    pub share_nodes_with_common_topics: bool,
}

/// Private set intersection (PSI) protocol for topic discovery.
///
/// PSI is a method for determining commonly held information between parties without revealing the
/// actual values. Here this is done by exchanging hashed versions of the topics so that each peer
/// can compute the intersection locally. At the end of a sucessful run both peers should end up
/// with the same set of intersecting topics.
///
/// We generate a salt that is unique per session by concatenating random bytes from both peers and
/// use it in the hash to prevent replay attacks. Additionally we add a single constant byte to the
/// salt depending on which peer the message is coming from so that bob cannot simply replay
/// alice's hashes as their own answer.
pub struct PsiHashDiscoveryProtocol<S, P, ID, N> {
    store: S,
    subscription: P,
    my_node_id: ID,
    remote_node_id: ID,
    config: Config,
    _marker: PhantomData<N>,
}

impl<S, P, ID, N> PsiHashDiscoveryProtocol<S, P, ID, N> {
    pub fn new(store: S, subscription: P, my_node_id: ID, remote_node_id: ID) -> Self {
        Self::with_config(
            store,
            subscription,
            my_node_id,
            remote_node_id,
            Config::default(),
        )
    }

    pub fn with_config(
        store: S,
        subscription: P,
        my_node_id: ID,
        remote_node_id: ID,
        config: Config,
    ) -> Self {
        Self {
            store,
            subscription,
            my_node_id,
            remote_node_id,
            config,
            _marker: PhantomData,
        }
    }

    /// Gather transport infos of nodes interested in common topics and always include our own.
    ///
    /// We don't share any node infos outside of this scope for privacy reasons.
    async fn gather_transport_infos(
        &self,
        topics: Vec<[u8; 32]>,
    ) -> Result<BTreeMap<ID, N::Transports>, PsiHashError<S, P, ID, N>>
    where
        S: AddressBookStore<ID, N>,
        P: LocalTopics,
        ID: PartialEq + Ord,
        N: NodeInfo<ID>,
    {
        let node_infos = if self.config.share_nodes_with_common_topics {
            let mut result = self
                .store
                .node_infos_by_topics(&topics)
                .await
                .map_err(PsiHashError::Store)?;

            // Always include our own transport info (in case it changed).
            let contains_our_info = result.iter().any(|info| info.id() == self.my_node_id);
            if !contains_our_info
                && let Some(my_node_info) = self
                    .store
                    .node_info(&self.my_node_id)
                    .await
                    .map_err(PsiHashError::Store)?
            {
                result.extend([my_node_info]);
            }

            result
        } else {
            self.store
                .all_node_infos()
                .await
                .map_err(PsiHashError::Store)?
        };

        // Assemble transport info results.
        let mut map = BTreeMap::new();
        for node_info in node_infos {
            if let Some(transport_info) = node_info.transports() {
                map.insert(node_info.id(), transport_info);
            }
        }
        Ok(map)
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
    type Error = PsiHashError<S, P, ID, N>;

    type Message = PsiHashMessage<ID, N>;

    async fn alice(
        &self,
        tx: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        rx: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> Result<DiscoveryResult<ID, N>, Self::Error> {
        let alice_salt_half = generate_salt_half();

        let message_1 = PsiHashMessage::AliceSaltHalf { alice_salt_half };
        tx.send(message_1).await.map_err(|_| PsiHashError::Sink)?;

        let message_2 = match rx.next().await {
            Some(val) => val.map_err(|_| PsiHashError::Stream)?,
            None => {
                return Err(PsiHashError::Stream);
            }
        };

        let PsiHashMessage::BobSaltHalfAndHashedData {
            bob_salt_half,
            topics_for_alice,
        } = message_2
        else {
            return Err(PsiHashError::UnexpectedMessage);
        };

        let my_topics: Vec<[u8; 32]> = self
            .subscription
            .topics()
            .await
            .map_err(PsiHashError::Subscription)?
            .into_iter()
            .collect();

        // Final salts.
        let alice_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &ALICE_SALT_BYTE);
        let bob_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &BOB_SALT_BYTE);

        // Alice computes intersection of their own work with what Bob sent them.
        let topics_intersection =
            compute_intersection(&my_topics, &topics_for_alice, &bob_final_salt)?;

        // Alice needs to hash their data with their salt and send to Bob so they can do the same.
        let topics_for_bob: HashSet<[u8; 32]> =
            HashSet::from_iter(hash_vector(&my_topics, &alice_final_salt)?.into_iter());

        tx.send(PsiHashMessage::AliceHashedData { topics_for_bob })
            .await
            .map_err(|_| PsiHashError::Sink)?;

        let message_4 = match rx.next().await {
            Some(val) => val.map_err(|_| PsiHashError::Stream)?,
            None => {
                return Err(PsiHashError::Stream);
            }
        };

        let PsiHashMessage::Nodes { transport_infos } = message_4 else {
            return Err(PsiHashError::UnexpectedMessage);
        };

        tx.send(PsiHashMessage::Nodes {
            transport_infos: self
                .gather_transport_infos(topics_intersection.clone().into_iter().collect::<Vec<_>>())
                .await?,
        })
        .await
        .map_err(|_| PsiHashError::Sink)?;

        Ok(DiscoveryResult {
            remote_node_id: self.remote_node_id.clone(),
            transport_infos,
            topics: topics_intersection,
        })
    }

    async fn bob(
        &self,
        tx: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        rx: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> Result<DiscoveryResult<ID, N>, Self::Error> {
        let Some(Ok(message_1)) = rx.next().await else {
            return Err(PsiHashError::Stream);
        };

        let PsiHashMessage::AliceSaltHalf { alice_salt_half } = message_1 else {
            return Err(PsiHashError::UnexpectedMessage);
        };
        let bob_salt_half = generate_salt_half();

        let alice_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &ALICE_SALT_BYTE);
        let bob_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &BOB_SALT_BYTE);

        let my_topics: Vec<[u8; 32]> = self
            .subscription
            .topics()
            .await
            .map_err(PsiHashError::Subscription)?
            .into_iter()
            .collect();

        let topics_for_alice: HashSet<[u8; 32]> =
            HashSet::from_iter(hash_vector(&my_topics, &bob_final_salt)?.into_iter());

        tx.send(PsiHashMessage::BobSaltHalfAndHashedData {
            bob_salt_half,
            topics_for_alice,
        })
        .await
        .map_err(|_| PsiHashError::Sink)?;

        let Some(Ok(message_3)) = rx.next().await else {
            return Err(PsiHashError::Stream);
        };

        let PsiHashMessage::AliceHashedData { topics_for_bob } = message_3 else {
            return Err(PsiHashError::UnexpectedMessage);
        };

        let topics_intersection =
            compute_intersection(&my_topics, &topics_for_bob, &alice_final_salt)?;

        tx.send(PsiHashMessage::Nodes {
            transport_infos: self
                .gather_transport_infos(topics_intersection.clone().into_iter().collect::<Vec<_>>())
                .await?,
        })
        .await
        .map_err(|_| PsiHashError::Sink)?;

        let Some(Ok(message_5)) = rx.next().await else {
            return Err(PsiHashError::Stream);
        };

        let PsiHashMessage::Nodes { transport_infos } = message_5 else {
            return Err(PsiHashError::UnexpectedMessage);
        };

        Ok(DiscoveryResult {
            remote_node_id: self.remote_node_id.clone(),
            transport_infos,
            topics: topics_intersection,
        })
    }
}

/// Compute intersection between our vector of topics and a hashed set from the peer as a set.
fn compute_intersection(
    local_topics: &[[u8; 32]],
    remote_hashes: &HashSet<[u8; 32]>,
    salt: &[u8; 65],
) -> Result<HashSet<[u8; 32]>, std::io::Error> {
    let local_topics_hashed = hash_vector(local_topics, salt)?;
    let mut intersection: HashSet<[u8; 32]> = HashSet::new();
    for (i, local_hash) in local_topics_hashed.iter().enumerate() {
        if remote_hashes.contains(local_hash) {
            intersection.insert(local_topics[i]);
        }
    }
    Ok(intersection)
}

/// Hash a vector of topics.
fn hash_vector(topics: &[[u8; 32]], salt: &[u8; 65]) -> Result<Vec<[u8; 32]>, std::io::Error> {
    topics.iter().map(|topic| hash(topic, salt)).collect()
}

/// Hash a topic with a salt using blake3.
fn hash(data: &[u8; 32], salt: &[u8; 65]) -> Result<[u8; 32], std::io::Error> {
    let mut hash = blake3::Hasher::new();
    hash.write_all(data)?;
    hash.write_all(salt)?;
    Ok(*hash.finalize().as_bytes())
}

/// Generate a random 32 byte array.
fn generate_salt_half() -> [u8; 32] {
    let mut generator = rng();
    let mut random_bytes: [u8; 32] = [0; 32];
    generator.fill_bytes(&mut random_bytes);
    random_bytes
}

/// Concatenates the two salt halves generated by each peer into a single salt unique for this
/// session.
///
/// We use [ALICE_SALT_BYTE] or [BOB_SALT_BYTE] to make the hashing unique in both directions. If
/// alice and bob used the exact same salt bob could fool alice by simply replaying alices hashes
/// back to her.
fn combine_salt(alice_salt_half: &[u8; 32], bob_salt_half: &[u8; 32], pair_byte: &u8) -> [u8; 65] {
    let mut output: [u8; 65] = [0; 65];
    output[0..32].copy_from_slice(alice_salt_half);
    output[32..64].copy_from_slice(bob_salt_half);
    output[64] = *pair_byte;
    output
}

#[derive(Debug, Error)]
pub enum PsiHashError<S, P, ID, N>
where
    S: AddressBookStore<ID, N>,
    P: LocalTopics,
{
    /// Error reading from the address book store.
    #[error("{0}")]
    Store(S::Error),

    /// Error reading topics from the provided subscription.
    #[error("{0}")]
    Subscription(P::Error),

    /// Peer sent us a message out of order.
    #[error("received unexpected message")]
    UnexpectedMessage,

    /// Cannot read from stream, connection is closed.
    #[error("stream closed unexpectedly")]
    Stream,

    /// Cannot write into stream, connection is closed.
    #[error("sink closed unexpectedly")]
    Sink,

    /// Hash error from BLAKE3 library.
    #[error(transparent)]
    Hash(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use futures_channel::mpsc;
    use futures_util::{SinkExt, StreamExt};
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    use crate::address_book::AddressBookStore;
    use crate::test_utils::{TestInfo, TestStore, TestSubscription};
    use crate::traits::DiscoveryProtocol;

    use super::{Config, PsiHashDiscoveryProtocol, PsiHashError, PsiHashMessage};

    #[tokio::test]
    async fn topic_discovery() {
        let rng = ChaCha20Rng::from_seed([1; 32]);

        let mut alice_subscription = TestSubscription::default();
        alice_subscription.topics.insert([1; 32]);
        alice_subscription.topics.insert([2; 32]);
        alice_subscription.topics.insert([98; 32]);
        alice_subscription.topics.insert([99; 32]);
        let alice_store = TestStore::new(rng.clone());

        let mut bob_subscription = TestSubscription::default();
        bob_subscription.topics.insert([2; 32]);
        bob_subscription.topics.insert([3; 32]);
        bob_subscription.topics.insert([99; 32]);
        bob_subscription.topics.insert([100; 32]);
        let bob_store = TestStore::new(rng.clone());

        let alice_protocol = PsiHashDiscoveryProtocol::new(alice_store, alice_subscription, 0, 1);
        let bob_protocol = PsiHashDiscoveryProtocol::new(bob_store, bob_subscription, 1, 0);

        let (mut alice_tx, alice_rx) = mpsc::channel(16);
        let (mut bob_tx, bob_rx) = mpsc::channel(16);

        let bob_handle = tokio::task::spawn(async move {
            let mut alice_rx = alice_rx.map(|message| Ok::<_, ()>(message));
            let Ok(result) = bob_protocol.bob(&mut bob_tx, &mut alice_rx).await else {
                panic!("running bob protocol failed");
            };
            result
        });

        // Wait until Alice has finished and store their results
        let mut bob_rx = bob_rx.map(|message| Ok::<_, ()>(message));
        let Ok(alice_result) = alice_protocol.alice(&mut alice_tx, &mut bob_rx).await else {
            panic!("running alice protocol failed");
        };

        // Wait until Bob has finished and store their results.
        let bob_result = bob_handle.await.expect("local task failure");

        let expected = HashSet::from_iter([[2; 32], [99; 32]]);
        assert_eq!(alice_result.topics, expected);
        assert_eq!(bob_result.topics, expected);
    }

    #[tokio::test]
    async fn topic_out_of_order_alice() {
        let rng = ChaCha20Rng::from_seed([1; 32]);

        let mut alice_subscription = TestSubscription::default();
        alice_subscription.topics.insert([1; 32]);
        alice_subscription.topics.insert([99; 32]);
        let alice_store = TestStore::new(rng.clone());

        let alice_protocol = PsiHashDiscoveryProtocol::new(alice_store, alice_subscription, 0, 1);

        let (mut alice_tx, _alice_rx) = mpsc::channel(16);
        let (mut bob_tx, bob_rx) = mpsc::channel(16);

        let bob_handle = tokio::task::spawn(async move {
            let _result = bob_tx
                .send(PsiHashMessage::AliceSaltHalf {
                    alice_salt_half: [0; 32],
                })
                .await;
        });

        let mut bob_rx = bob_rx.map(|message| Ok::<_, ()>(message));
        let alice_result = alice_protocol.alice(&mut alice_tx, &mut bob_rx).await;
        let _bob_result = bob_handle.await;
        assert!(matches!(alice_result, Err(PsiHashError::UnexpectedMessage)));
    }

    #[tokio::test]
    async fn topic_out_of_order_bob() {
        let rng = ChaCha20Rng::from_seed([1; 32]);

        let mut bob_subscription = TestSubscription::default();
        bob_subscription.topics.insert([1; 32]);
        bob_subscription.topics.insert([99; 32]);
        let bob_store = TestStore::new(rng.clone());

        let bob_protocol = PsiHashDiscoveryProtocol::new(bob_store, bob_subscription, 0, 1);

        let (mut bob_tx, _) = mpsc::channel(16);
        let (mut alice_tx, alice_rx) = mpsc::channel(16);

        tokio::task::spawn(async move {
            let _result = alice_tx
                .send(PsiHashMessage::AliceHashedData {
                    topics_for_bob: HashSet::new(),
                })
                .await;
        });

        let mut alice_rx = alice_rx.map(|message| Ok::<_, ()>(message));
        let bob_result = bob_protocol.bob(&mut bob_tx, &mut alice_rx).await;
        assert!(matches!(bob_result, Err(PsiHashError::UnexpectedMessage)));
    }

    #[tokio::test]
    async fn transport_info() {
        let mut rng = ChaCha20Rng::from_seed([1; 32]);

        // Alice, Bob and Charlie share the same topic [1; 32] while only Bob and Daphne share
        // topic [2; 32]. Alice should _not_ learn transport info about Daphne and only about
        // Charlie.
        //
        // 0: Alice:   [1; 32]
        // 1: Bob:     [1; 32] [2; 32]
        // 2: Charlie: [1; 32]
        // 3: Daphne:          [2; 32]

        // Prepare Alice.
        let mut alice_subscription = TestSubscription::default();
        alice_subscription.topics.insert([1; 32]);

        let alice_store = TestStore::new(rng.clone());

        alice_store
            .insert_node_info(TestInfo::new(0).with_random_address(&mut rng))
            .await
            .unwrap();
        alice_store
            .set_topics(0, HashSet::from_iter([[1; 32]]))
            .await
            .unwrap();

        // Prepare Bob.
        let mut bob_subscription = TestSubscription::default();
        bob_subscription.topics.insert([1; 32]);
        bob_subscription.topics.insert([2; 32]);

        let bob_store = TestStore::new(rng.clone());

        bob_store
            .insert_node_info(TestInfo::new(1).with_random_address(&mut rng))
            .await
            .unwrap();
        bob_store
            .set_topics(1, HashSet::from_iter([[1; 32], [2; 32]]))
            .await
            .unwrap();

        // "Charlie"
        bob_store
            .insert_node_info(TestInfo::new(2).with_random_address(&mut rng))
            .await
            .unwrap();
        bob_store
            .set_topics(2, HashSet::from_iter([[1; 32]]))
            .await
            .unwrap();

        // "Daphne"
        bob_store
            .insert_node_info(TestInfo::new(3).with_random_address(&mut rng))
            .await
            .unwrap();
        bob_store
            .set_topics(3, HashSet::from_iter([[2; 32]]))
            .await
            .unwrap();

        let config = Config {
            share_nodes_with_common_topics: true,
        };

        let alice_protocol = PsiHashDiscoveryProtocol::with_config(
            alice_store,
            alice_subscription,
            0,
            1,
            config.clone(),
        );
        let bob_protocol =
            PsiHashDiscoveryProtocol::with_config(bob_store, bob_subscription, 1, 0, config);

        let (mut alice_tx, alice_rx) = mpsc::channel(16);
        let (mut bob_tx, bob_rx) = mpsc::channel(16);

        let bob_handle = tokio::task::spawn(async move {
            let mut alice_rx = alice_rx.map(|message| Ok::<_, ()>(message));
            let Ok(result) = bob_protocol.bob(&mut bob_tx, &mut alice_rx).await else {
                panic!("running bob protocol failed");
            };
            result
        });

        let mut bob_rx = bob_rx.map(|message| Ok::<_, ()>(message));
        let Ok(alice_result) = alice_protocol.alice(&mut alice_tx, &mut bob_rx).await else {
            panic!("running alice protocol failed");
        };

        // Wait until Bob has finished.
        let bob_result = bob_handle.await.expect("local task failure");

        // Alice learned about Charlie.
        assert!(alice_result.transport_infos.contains_key(&1)); // Bob
        assert!(alice_result.transport_infos.contains_key(&2)); // Charlie
        assert_eq!(alice_result.transport_infos.len(), 2);

        // Alice did _not_ learn about Daphne.
        assert!(!alice_result.transport_infos.contains_key(&3));

        // Bob only got the info of Alice.
        assert!(bob_result.transport_infos.contains_key(&0)); // Alice
        assert_eq!(bob_result.transport_infos.len(), 1);
    }
}
