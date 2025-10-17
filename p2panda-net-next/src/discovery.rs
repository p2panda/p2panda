// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused)]
use std::collections::HashSet;
use std::collections::btree_map::IterMut;
use std::hash::Hash;
use std::ops::Mul;

use curve25519_dalek::ristretto::CompressedRistretto;
use curve25519_dalek::{RistrettoPoint, Scalar};
use futures_lite::io::WriteVectoredFuture;
use rand_core::OsRng;
use sha2::Digest;
use sha2::{Sha256, Sha512};
use thiserror;
use tokio::sync::mpsc;

type Sha256Hash = [u8; 32];

const ALICE_SALT: [u8; 1] = [0];
const BOB_SALT: [u8; 1] = [1];

// pub struct RistrettoHashable(RistrettoPoint);

// impl Hash for RistrettoHashable {
//     fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
//         self.0.compress().to_bytes().hash(state)
//     }
// }

// impl RistrettoHashable {
//     fn to_bytes(&self) -> [u8;32] {
//         self.0.compress().to_bytes()
//     }
// }

pub enum DiscoveryMessage {
    AliceInitialToBob(Vec<CompressedRistretto>),
    BobReply(Vec<Sha256Hash>, Vec<CompressedRistretto>),
    AliceFinalToBob(Vec<Sha256Hash>),
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error(transparent)]
    Send(#[from] mpsc::error::SendError<DiscoveryMessage>),

    #[error("receive channel unexpectedly closed")]
    Receive,

    #[error("received unexpected message")]
    UnexpectedMessage,

    #[error("not canonical encoding of ristretto point")]
    InvalidRistrettoEncoding,
}

pub fn to_ristretto(data: &[Vec<u8>]) -> Vec<CompressedRistretto> {
    data.iter()
        .map(|item| RistrettoPoint::hash_from_bytes::<Sha512>(item).compress())
        .collect()
}

pub fn scalar_mult(
    scalar: Scalar,
    data: &[CompressedRistretto],
) -> Result<Vec<CompressedRistretto>, DiscoveryError> {
    data.iter()
        .map(|item| {
            match item
                .decompress()
                .ok_or(DiscoveryError::InvalidRistrettoEncoding)
            {
                Ok(item) => Ok((item * scalar).compress()),
                Err(err) => Err(err),
            }
        })
        .collect()
}

pub fn hash(data: &CompressedRistretto, salt: impl AsRef<[u8]>) -> Sha256Hash {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    let bytes = data.to_bytes();
    hasher.update(&bytes);
    hasher.finalize().into()
}

pub async fn alice_protocol(
    alice_topics: &[Vec<u8>],
    tx: mpsc::Sender<DiscoveryMessage>,
    mut rx: mpsc::Receiver<DiscoveryMessage>,
) -> Result<Vec<Vec<u8>>, DiscoveryError> {
    let mut rng = OsRng;
    let scalar = Scalar::random(&mut rng);

    let a_mixed = scalar_mult(scalar, &to_ristretto(alice_topics))?;

    let message_1 = DiscoveryMessage::AliceInitialToBob(a_mixed);
    tx.send(message_1).await?;
    let message_2 = rx.recv().await.ok_or(DiscoveryError::Receive)?;

    let DiscoveryMessage::BobReply(bob_hashed_complete_mix, bob_half) = message_2 else {
        return Err(DiscoveryError::UnexpectedMessage);
    };

    // assert bob's complete length = our original
    assert_eq!(bob_hashed_complete_mix.len(), alice_topics.len());

    let complete_hashed_set: HashSet<[u8; 32]> = {
        let complete = scalar_mult(scalar, &bob_half)?;
        let complete_hashed = complete.iter().map(|i| hash(i, BOB_SALT));
        HashSet::from_iter(complete_hashed)
    };

    let bob_hashed_set: HashSet<[u8; 32]> =
        HashSet::from_iter(bob_hashed_complete_mix.iter().cloned());

    let intersection = bob_hashed_set.intersection(&complete_hashed_set);

    let result = intersection
        .map(|intersection_hash| {
            bob_hashed_complete_mix
                .iter()
                .enumerate()
                .find_map(|(index, item)| {
                    if item == intersection_hash {
                        Some(alice_topics.get(index).expect(
                            "alice_topics should be aligned in with bob_hashed_complete_mix",
                        ).clone())
                    } else {
                        None
                    }
                })
                .expect("we will always find an item locally that was in our intersection")
        })
        .collect();
    Ok(result)
}

pub async fn bob_protocol(
    bob_topics: &[Vec<u8>],
    tx: mpsc::Sender<DiscoveryMessage>,
    mut rx: mpsc::Receiver<DiscoveryMessage>,
) -> Result<Vec<Vec<u8>>, DiscoveryError> {
    println!("bob start");
    let message_1 = rx.recv().await.ok_or(DiscoveryError::Receive)?;
    println!("bob receive 1");

    let DiscoveryMessage::AliceInitialToBob(alice_half) = message_1 else {
        return Err(DiscoveryError::UnexpectedMessage);
    };

    let mut rng = OsRng;
    let scalar = Scalar::random(&mut rng);

    let b_mixed = scalar_mult(scalar, &to_ristretto(bob_topics))?;

    let complete = scalar_mult(scalar, &alice_half)?;
    let complete_hashed = complete.iter().map(|i| hash(i, BOB_SALT)).collect();

    // 
    println!("bob send 2");
    tx.send(DiscoveryMessage::BobReply(complete_hashed, b_mixed)).await?;
    println!("bob wait for 3");
    // let message_3 = rx.recv().await.ok_or(DiscoveryError::Receive)?;
    println!("bob FAKE receive 3");

    Ok(vec![])

    // todo!()
    // let DiscoveryMessage::AliceInitialToBob(alice_half) = message_1 else {
    //     return Err(DiscoveryError::UnexpectedMessage);
    // };



    // // after 3 received
    // let complete_hashed_set: HashSet<[u8; 32]> = HashSet::from_iter(complete_hashed);

    // let message_3 = DiscoveryMessage::AliceInitialToBob(b_mixed);
    // tx.send(message_1).await?;
    // let message_3 = rx.recv().await.ok_or(DiscoveryError::Receive)?;

    // let DiscoveryMessage::BobReply(bob_hashed_complete_mix, bob_half) = message_3 else {
    //     return Err(DiscoveryError::UnexpectedMessage);
    // };

    // // assert bob's complete length = our original
    // assert_eq!(bob_hashed_complete_mix.len(), alice_topics.len());

    // let complete_hashed_set: HashSet<[u8; 32]> = {
    //     let complete = scalar_mult(scalar, &bob_half)?;
    //     let complete_hashed = complete.iter().map(|i| hash(i, BOB_SALT));
    //     HashSet::from_iter(complete_hashed)
    // };

    // let bob_hashed_set: HashSet<[u8; 32]> =
    //     HashSet::from_iter(bob_hashed_complete_mix.iter().cloned());

    // let intersection = bob_hashed_set.intersection(&complete_hashed_set);

    // let result = intersection
    //     .map(|intersection_hash| {
    //         bob_hashed_complete_mix
    //             .iter()
    //             .enumerate()
    //             .find_map(|(index, item)| {
    //                 if item == intersection_hash {
    //                     Some(alice_topics.get(index).expect(
    //                         "alice_topics should be aligned in with bob_hashed_complete_mix",
    //                     ).clone())
    //                 } else {
    //                     None
    //                 }
    //             })
    //             .expect("we will always find an item locally that was in our intersection")
    //     })
    //     .collect();
    // Ok(result)
}

#[cfg(test)]
mod tests {
    use std::vec;

    use curve25519_dalek::Scalar;
    use rand_core::OsRng;
    use tokio::sync::mpsc;

    use crate::discovery::bob_protocol;

    use super::{DiscoveryMessage, alice_protocol, scalar_mult, to_ristretto};

    // show our math is commutative
    #[test]
    fn test_scalar_mult() {
        let mut rng = OsRng;
        let a_scalar = Scalar::random(&mut rng);
        let b_scalar = Scalar::random(&mut rng);

        let test_data = [vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];

        let a_mixed = scalar_mult(a_scalar, &to_ristretto(&test_data)).unwrap();
        let b_mixed = scalar_mult(b_scalar, &to_ristretto(&test_data)).unwrap();

        let b_of_a_mixed = scalar_mult(b_scalar, &a_mixed).unwrap();
        let a_of_b_mixed = scalar_mult(a_scalar, &b_mixed).unwrap();

        assert_eq!(b_of_a_mixed, a_of_b_mixed);
    }

    #[tokio::test]
    async fn alice_is_happy() {
        let (alice_tx, alice_rx) = mpsc::channel::<DiscoveryMessage>(16);
        let (bob_tx, bob_rx) = mpsc::channel::<DiscoveryMessage>(16);
        let alice_topics = [vec![1,2,3],vec![4,5,6]];
        let bob_topics = [vec![4,5,6], vec![7,8,9]];

        let bob_handle = tokio::task::spawn(async move {
            bob_protocol(&bob_topics, alice_tx, bob_rx).await
        });

        let alice_result = alice_protocol(&alice_topics, bob_tx, alice_rx).await.unwrap();
        let bob_result = bob_handle.await.unwrap().unwrap();
        assert_eq!(alice_result, vec![vec![4,5,6]]);
        assert_eq!(bob_result, Vec::<Vec<u8>>::new());
    }
}
