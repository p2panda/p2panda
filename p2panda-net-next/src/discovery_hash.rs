// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused)]

use std::collections::{hash_set::Intersection, HashSet};
use std::collections::btree_map::IterMut;

use argon2::{
    Argon2,
    password_hash::{self, PasswordHash, PasswordHasher, Salt, SaltString, rand_core::OsRng},
};
use thiserror::{self};
use tokio::sync::mpsc;

const ALICE_SALT: [u8; 1] = [0];
const BOB_SALT: [u8; 1] = [1];

pub enum DiscoveryMessage {
    AliceSetSecretHalf {
        salt_half: SaltString,
    },
    BobSecretAndHashedData {
        salt_half: SaltString,
        bob_hashed_for_alice: Vec<String>,
    },
    AliceHashedData {
        alice_hashed_for_bob: Vec<String>,
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

    #[error("not canonical encoding of ristretto point")]
    InvalidRistrettoEncoding,

    // todo replace with transparent pasword_hash::Error type, had dyn trait problem
    #[error("error hashing")]
    HashFunction,
}

pub fn hash(data: &String, salt: &SaltString) -> Result<String, DiscoveryError> {
    let argon2 = Argon2::default();
    match argon2.hash_password(data.as_bytes(), salt) {
        Ok(password_hash) => Ok(password_hash.to_string()),
        Err(_) => Err(DiscoveryError::HashFunction),
    }
}

pub fn combine_salt(
    alice_salt_half: &SaltString,
    bob_salt_half: &SaltString,
    pair_byte: &[u8; 1],
) -> Result<SaltString, DiscoveryError> {
    let mut alice_b64: Vec<u8> = vec![];
    let alice_b64 = alice_salt_half
        .decode_b64(&mut alice_b64)
        .or(Err(DiscoveryError::HashFunction))?;
    let mut bob_b64: Vec<u8> = vec![];
    let bob_b64 = bob_salt_half
        .decode_b64(&mut bob_b64)
        .or(Err(DiscoveryError::HashFunction))?;
    // let concat_b64 = vec![alice_b64, bob_b64];
    let mut concat_b64 = alice_b64.to_owned();
    concat_b64.extend_from_slice(bob_b64);
    concat_b64.extend_from_slice(pair_byte);
    // .concat()?;
    let concat_str: String = concat_b64.to_owned().try_into().unwrap();
    SaltString::from_b64(&concat_str).or(Err(DiscoveryError::HashFunction))
}

pub async fn alice_protocol(
    alice_topics: &[String],
    tx: mpsc::Sender<DiscoveryMessage>,
    mut rx: mpsc::Receiver<DiscoveryMessage>,
) -> Result<Vec<String>, DiscoveryError> {
    let alice_salt_half = SaltString::generate(&mut OsRng);

    let message_1 = DiscoveryMessage::AliceSetSecretHalf {
        salt_half: alice_salt_half.clone(),
    };
    tx.send(message_1).await?;
    let message_2 = rx.recv().await.ok_or(DiscoveryError::Receive)?;

    let DiscoveryMessage::BobSecretAndHashedData {
        salt_half: bob_salt_half,
        bob_hashed_for_alice,
    } = message_2
    else {
        return Err(DiscoveryError::UnexpectedMessage);
    };

    // final salts
    let alice_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &ALICE_SALT)?;
    let bob_final_salt = combine_salt(&alice_salt_half, &bob_salt_half, &BOB_SALT)?;

    let local_bob_hashed = alice_topics
        .into_iter()
        .map(|topic| hash(topic, &bob_final_salt))
        .collect::<Result<Vec<String>, DiscoveryError>>()?;
    
    let bob_hashed_for_alice_map: HashSet<String> = HashSet::from_iter(bob_hashed_for_alice.iter().cloned());

    // alice computes intersection of her own work with what bob sent her
    let mut alice_intersection: Vec<String> = vec![];
    for (i, local_hash) in local_bob_hashed.iter().enumerate() {
        if bob_hashed_for_alice_map.contains(local_hash) {
            alice_intersection.push(alice_topics[i].clone());
        }
    }

    // now alice needs to hash her data with her salt and send to bob so he can do the same

    let alice_hashed_for_bob: Vec<String> = alice_topics
    .into_iter()
    .map(|topic| hash(topic, &alice_final_salt))
    .collect::<Result<Vec<String>, DiscoveryError>>()?;

    tx.send(DiscoveryMessage::AliceHashedData { alice_hashed_for_bob }).await?;

    Ok(alice_intersection)
}
