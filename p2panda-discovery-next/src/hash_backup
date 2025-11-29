// SPDX-License-Identifier: MIT OR Apache-2.0

// #![allow(unused)]

use std::collections::HashSet;
use std::io::Write;

use blake3;
use rand::{RngCore, thread_rng};
use thiserror::{self};
use tokio::sync::mpsc;

const ALICE_SALT_BIT: [u8; 1] = [0];
const BOB_SALT_BIT: [u8; 1] = [1];

pub enum DiscoveryMessage {
    AliceSecretHalf {
        salt_half: Vec<u8>,
    },
    BobSecretHalfAndHashedData {
        salt_half: Vec<u8>,
        bob_hashed_for_alice: Vec<[u8; 32]>,
    },
    AliceHashedData {
        alice_hashed_for_bob: Vec<[u8; 32]>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error(transparent)]
    Send(#[from] mpsc::error::SendError<DiscoveryMessage>),

    #[error("receive channel unexpectedly closed")]
    Receive,

    #[error("received unexpected message")]
    UnexpectedMessage,

    #[error(transparent)]
    HashIO(#[from] std::io::Error)
}

pub fn hash(data: &Vec<u8>, salt: &Vec<u8>) -> Result<blake3::Hash, DiscoveryError> {
    let mut hash = blake3::Hasher::new();
    hash.write_all(data)?;
    hash.write_all(salt)?;
    Ok(hash.finalize())
}

pub fn generate_salt() -> Vec<u8> {
    let mut generator = thread_rng();

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

#[allow(unused)]
pub async fn alice_protocol(
    alice_topics: &[Vec<u8>],
    tx: mpsc::Sender<DiscoveryMessage>,
    mut rx: mpsc::Receiver<DiscoveryMessage>,
) -> Result<Vec<Vec<u8>>, DiscoveryError> {
    let alice_salt_half = generate_salt();

    let message_1 = DiscoveryMessage::AliceSecretHalf {
        salt_half: alice_salt_half.clone(),
    };
    tx.send(message_1).await?;
    let message_2 = rx.recv().await;
    let message_2 = match message_2 {
        Some(val) => val,
        None => {
            return Err(DiscoveryError::Receive);
        }
    };

    let DiscoveryMessage::BobSecretHalfAndHashedData {
        salt_half: bob_salt_half,
        bob_hashed_for_alice,
    } = message_2
    else {
        return Err(DiscoveryError::UnexpectedMessage);
    };

    // final salts
    let alice_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &ALICE_SALT_BIT);
    let bob_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &BOB_SALT_BIT);

    let local_bob_hashed: Vec<[u8; 32]> = alice_topics
        .into_iter()
        .map(|topic| hash(topic, &bob_final_salt))
        .collect::<Result<Vec<blake3::Hash>, _>>()?
        .into_iter()
        .map(|h| h.as_bytes().clone())
        .collect();

    let bob_hashed_for_alice_map: HashSet<[u8; 32]> =
        HashSet::from_iter(bob_hashed_for_alice.iter().cloned());

    // alice computes intersection of her own work with what bob sent her
    let mut alice_intersection: Vec<Vec<u8>> = vec![];
    for (i, local_hash) in local_bob_hashed.iter().enumerate() {
        if bob_hashed_for_alice_map.contains(local_hash) {
            alice_intersection.push(alice_topics[i].clone());
        }
    }

    // now alice needs to hash her data with her salt and send to bob so he can do the same

    let alice_hashed_for_bob: Vec<[u8; 32]> = alice_topics
        .into_iter()
        .map(|topic| hash(topic, &alice_final_salt))
        .collect::<Result<Vec<blake3::Hash>, _>>()?
        .into_iter()
        .map(|h| h.as_bytes().clone())
        .collect();

    tx.send(DiscoveryMessage::AliceHashedData {
        alice_hashed_for_bob,
    })
    .await?;

    Ok(alice_intersection)
}

#[allow(unused)]
pub async fn bob_protocol(
    bob_topics: &[Vec<u8>],
    tx: mpsc::Sender<DiscoveryMessage>,
    mut rx: mpsc::Receiver<DiscoveryMessage>,
) -> Result<Vec<Vec<u8>>, DiscoveryError> {
    let message_1 = rx.recv().await.ok_or(DiscoveryError::Receive)?;
    let DiscoveryMessage::AliceSecretHalf {
        salt_half: alice_salt_half,
    } = message_1
    else {
        return Err(DiscoveryError::UnexpectedMessage);
    };

    let bob_salt_half = generate_salt();

    // final salts
    let alice_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &ALICE_SALT_BIT);
    let bob_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &BOB_SALT_BIT);

    let bob_hashed_for_alice = bob_topics
        .into_iter()
        .map(|topic| hash(topic, &bob_final_salt))
        .collect::<Result<Vec<blake3::Hash>, _>>()?
        .into_iter()
        .map(|h| h.as_bytes().clone())
        .collect();

    tx.send(DiscoveryMessage::BobSecretHalfAndHashedData {
        salt_half: bob_salt_half,
        bob_hashed_for_alice,
    })
    .await?;
    let message_3 = rx.recv().await.ok_or(DiscoveryError::Receive)?;

    let DiscoveryMessage::AliceHashedData {
        alice_hashed_for_bob,
    } = message_3
    else {
        return Err(DiscoveryError::UnexpectedMessage);
    };

    let alice_hashed_for_bob_map: HashSet<[u8; 32]> =
        HashSet::from_iter(alice_hashed_for_bob.iter().cloned());

    let local_alice_hashed: Vec<[u8; 32]> = bob_topics
        .into_iter()
        .map(|topic| hash(topic, &alice_final_salt))
        .collect::<Result<Vec<blake3::Hash>, _>>()?
        .into_iter()
        .map(|h| h.as_bytes().clone())
        .collect();

    // bob computes intersection
    let mut bob_intersection: Vec<Vec<u8>> = vec![];
    for (i, local_hash) in local_alice_hashed.iter().enumerate() {
        if alice_hashed_for_bob_map.contains(local_hash) {
            bob_intersection.push(bob_topics[i].clone());
        }
    }

    Ok(bob_intersection)
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::{DiscoveryMessage, alice_protocol, bob_protocol};

    #[tokio::test]
    async fn alice_is_happy_hashed() {
        let (alice_tx, alice_rx) = mpsc::channel::<DiscoveryMessage>(16);
        let (bob_tx, bob_rx) = mpsc::channel::<DiscoveryMessage>(16);
        let alice_topics = vec![vec![0, 1, 2], vec![3, 4, 5]];
        let bob_topics = vec![vec![3, 4, 5], vec![6, 7, 8]];

        let bob_handle =
            tokio::task::spawn(async move { bob_protocol(&bob_topics, alice_tx, bob_rx).await });

        let alice_result = alice_protocol(&alice_topics, bob_tx, alice_rx).await;

        let bob_result = bob_handle.await.unwrap().unwrap();
        let alice_result = alice_result.unwrap();
    }
}
