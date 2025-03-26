// SPDX-License-Identifier: MIT OR Apache-2.0

//! Two-Party Secure Messaging (2SM) Key Agreement Protocol.
use std::collections::HashMap;
use std::marker::PhantomData;

use p2panda_core::cbor::{DecodeError, EncodeError, decode_cbor, encode_cbor};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::hpke::{HpkeCiphertext, HpkeError, hpke_open, hpke_seal};
use crate::crypto::x25519::{PublicKey, SecretKey, X25519Error};
use crate::crypto::{Rng, RngError};
use crate::key_bundle::{LongTermKeyBundle, OneTimeKeyBundle};
use crate::key_manager::KeyManager;
use crate::traits::{IdentityManager, KeyBundle, PreKeyManager};
use crate::two_party::{X3DHCiphertext, X3DHError, x3dh_decrypt, x3dh_encrypt};

/// Two-Party Secure Messaging (2SM) Key Agreement Protocol as specified in the paper "Key
/// Agreement for Decentralized Secure Group Messaging with Strong Security Guarantees" (2020).
///
/// 2SM is used for key-agreement as part of the DCGKA protocol allowing all members to learn about
/// the "seed" for establishing new secret state. 2SM is pair-wise between all members of an
/// encrypted group. p2panda uses 2SM for both "data-" and "message encryption" schemes.
///
/// ## Cost of key-agreements
///
/// To make a group aware of a new secret key we could encrypt the secret pairwise with public-key
/// encryption (PKE) towards each member of the group, resulting in an O(n^2) overhead as every
/// member needs to share their secret with every other. The paper proposes an alternative approach
/// with 2SM where a member prepares the next encryption keys not only for themselves but also for
/// the other party, resulting in a more optimal O(n) cost when rotating keys. This allows us to
/// "heal" the group in less steps after a member is removed.
///
/// <https://eprint.iacr.org/2020/1281.pdf>
pub struct TwoParty<KEY, KB> {
    _marker: PhantomData<(KEY, KB)>,
}

pub type OneTimeTwoParty = TwoParty<KeyManager, OneTimeKeyBundle>;

pub type LongTermTwoParty = TwoParty<KeyManager, LongTermKeyBundle>;

/// State of 2SM session between a sending- ("our") and receiving member ("their").
///
/// All 2SM methods are expressed as "pure functions" without any side-effects, returning an
/// updated state object. This allows applications to be more crash-resiliant, persisting the final
/// state only when all processes have successfully completed.
///
/// The state is serializable and can be used to persist 2SM sessions.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(Clone))]
pub struct TwoPartyState<KB: KeyBundle> {
    /// Index of key we will use during next send. The receiver can use the public key and refer to
    /// it through that index when they want to encrypt a message back to us.
    pub our_next_key_index: u64,

    /// Index of the last key which was used by the other peer to encrypt a message towards us. We
    /// keep it around to understand which secret keys we can remove.
    pub our_min_key_index: u64,

    /// List of all secret keys we generated ourselves. We sent the public counterpart to the other
    /// peer.
    pub our_secret_keys: HashMap<u64, SecretKey>,

    /// Last secret key the other peer generated for us. This is part of the 2SM protocol and an
    /// optimization where the remote end can _also_ generate secrets for us.
    pub our_received_secret_key: Option<SecretKey>,

    /// Which key we use to decrypt the next incoming message.
    pub their_next_key_used: KeyUsed,

    /// Public identity key of the other peer. We use it to verify the signature of their prekey.
    pub their_identity_key: PublicKey,

    /// Key-material we need to encrypt the first message with the help of X3DH and prekeys.
    pub their_prekey_bundle: Option<KB>,

    /// Last known public key of the other peer. We use it to encrypt a message towards them.
    pub their_public_key: Option<PublicKey>,
}

// Public methods.

impl<KEY, KB> TwoParty<KEY, KB>
where
    KEY: IdentityManager<KEY::State> + PreKeyManager,
    KB: KeyBundle,
{
    pub fn init(their_prekey_bundle: KB) -> TwoPartyState<KB> {
        TwoPartyState {
            our_next_key_index: 1,
            our_min_key_index: 1,
            our_secret_keys: HashMap::new(),
            our_received_secret_key: None,
            their_identity_key: *their_prekey_bundle.identity_key(),
            their_public_key: None,
            their_next_key_used: KeyUsed::PreKey,
            their_prekey_bundle: Some(their_prekey_bundle),
        }
    }

    pub fn send(
        y: TwoPartyState<KB>,
        y_manager: &KEY::State,
        plaintext: &[u8],
        rng: &Rng,
    ) -> TwoPartyResult<(TwoPartyState<KB>, TwoPartyMessage)> {
        let (for_us, for_them) = Self::generate_keys(rng)?;

        let plaintext_message = TwoPartyPlaintext {
            plaintext: plaintext.to_vec(),
            receiver_new_secret: for_them.their_new_secret.clone(),
            sender_new_public_key: for_them.our_new_public_key,
            sender_next_index: y.our_next_key_index,
        };
        let plaintext_bytes = plaintext_message.to_bytes()?;

        let (mut y_i, ciphertext) = Self::encrypt(y, y_manager, &plaintext_bytes, rng)?;

        let message = TwoPartyMessage {
            ciphertext,
            key_used: y_i.their_next_key_used,
        };

        y_i.our_secret_keys
            .insert(y_i.our_next_key_index, for_us.our_new_secret);
        y_i.our_next_key_index += 1;

        y_i.their_public_key = Some(for_us.their_new_public_key);

        y_i.their_next_key_used = KeyUsed::ReceivedKey;

        Ok((y_i, message))
    }

    pub fn receive(
        y: TwoPartyState<KB>,
        y_manager: KEY::State,
        message: TwoPartyMessage,
    ) -> TwoPartyResult<(TwoPartyState<KB>, KEY::State, Vec<u8>)> {
        let (mut y_i, y_manager_i, plaintext_bytes) =
            Self::decrypt(y, y_manager, message.ciphertext, message.key_used)?;
        let plaintext_message = TwoPartyPlaintext::from_bytes(&plaintext_bytes)?;

        y_i.their_public_key = Some(plaintext_message.sender_new_public_key);
        y_i.their_next_key_used = KeyUsed::OwnKey(plaintext_message.sender_next_index);
        y_i.our_received_secret_key = Some(plaintext_message.receiver_new_secret);

        Ok((y_i, y_manager_i, plaintext_message.plaintext))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)]
pub enum KeyUsed {
    /// Previously published keys ("prekeys") for X3DH.
    PreKey,

    /// Key the receiving peer received last time from the sending peer.
    ReceivedKey,

    /// Key the receiving peer generated themselves at some time. We can refer to the exact key by
    /// the it's index.
    OwnKey(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoPartyMessage {
    ciphertext: TwoPartyCiphertext,
    key_used: KeyUsed,
}

pub type TwoPartyResult<T> = Result<T, TwoPartyError>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TwoPartyCiphertext {
    PreKey(X3DHCiphertext),
    Hpke(HpkeCiphertext),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoPartyPlaintext {
    plaintext: Vec<u8>,
    receiver_new_secret: SecretKey,
    sender_new_public_key: PublicKey,
    sender_next_index: u64,
}

impl TwoPartyPlaintext {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        decode_cbor(bytes)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        encode_cbor(&self)
    }
}

// Private methods.

impl<KEY, KB> TwoParty<KEY, KB>
where
    KEY: IdentityManager<KEY::State> + PreKeyManager,
    KB: KeyBundle,
{
    fn encrypt(
        mut y: TwoPartyState<KB>,
        y_manager: &KEY::State,
        plaintext: &[u8],
        rng: &Rng,
    ) -> TwoPartyResult<(TwoPartyState<KB>, TwoPartyCiphertext)> {
        let ciphertext = match &y.their_public_key {
            None => {
                // Establish secret via X3DH from prekey when no secret is given yet.
                let their_prekey_bundle = y
                    .their_prekey_bundle
                    .take()
                    .ok_or(TwoPartyError::PreKeyReuse)?;
                let ciphertext = x3dh_encrypt(
                    plaintext,
                    KEY::identity_secret(y_manager),
                    &their_prekey_bundle,
                    rng,
                )?;
                TwoPartyCiphertext::PreKey(ciphertext)
            }
            Some(their_public_key) => {
                let ciphertext = hpke_seal(their_public_key, None, None, plaintext, rng)?;
                TwoPartyCiphertext::Hpke(ciphertext)
            }
        };

        Ok((y, ciphertext))
    }

    fn decrypt(
        mut y: TwoPartyState<KB>,
        y_manager: KEY::State,
        ciphertext: TwoPartyCiphertext,
        key_used: KeyUsed,
    ) -> TwoPartyResult<(TwoPartyState<KB>, KEY::State, Vec<u8>)> {
        let (y_manager_i, plaintext) = match key_used {
            KeyUsed::PreKey => {
                let TwoPartyCiphertext::PreKey(ciphertext) = ciphertext else {
                    return Err(TwoPartyError::InvalidCiphertextType);
                };

                // If the underlying key manager provides a one-time secret, we use it here.
                let (y_manager_i, onetime_secret) = match ciphertext.onetime_prekey_id {
                    Some(onetime_prekey_id) => {
                        let (y_manager_i, onetime_secret) =
                            KEY::use_onetime_secret(y_manager, onetime_prekey_id)
                                .map_err(|_| TwoPartyError::PreKeyReuse)?;
                        (y_manager_i, onetime_secret)
                    }
                    None => (y_manager, None),
                };

                let plaintext = x3dh_decrypt(
                    &ciphertext,
                    KEY::identity_secret(&y_manager_i),
                    KEY::prekey_secret(&y_manager_i),
                    onetime_secret.as_ref(),
                )?;

                (y_manager_i, plaintext)
            }
            KeyUsed::ReceivedKey => {
                let TwoPartyCiphertext::Hpke(ciphertext) = ciphertext else {
                    return Err(TwoPartyError::InvalidCiphertextType);
                };

                let Some(our_received_secret_key) = &y.our_received_secret_key else {
                    return Err(TwoPartyError::UnknownSecretUsed(0));
                };

                let plaintext = hpke_open(&ciphertext, our_received_secret_key, None, None)?;
                (y_manager, plaintext)
            }
            KeyUsed::OwnKey(index) => {
                let TwoPartyCiphertext::Hpke(ciphertext) = ciphertext else {
                    return Err(TwoPartyError::InvalidCiphertextType);
                };

                let plaintext = match y.our_secret_keys.get(&index) {
                    Some(secret) => hpke_open(&ciphertext, secret, None, None)?,
                    None => return Err(TwoPartyError::UnknownSecretUsed(index)),
                };

                for i in y.our_min_key_index..index {
                    y.our_secret_keys.remove(&i);
                }
                y.our_min_key_index = index;

                (y_manager, plaintext)
            }
        };

        Ok((y, y_manager_i, plaintext))
    }
}

struct NewKeysForUs {
    our_new_secret: SecretKey,
    their_new_public_key: PublicKey,
}

struct NewKeysForThem {
    our_new_public_key: PublicKey,
    their_new_secret: SecretKey,
}

impl<KEY, KB> TwoParty<KEY, KB> {
    fn generate_keys(rng: &Rng) -> TwoPartyResult<(NewKeysForUs, NewKeysForThem)> {
        let our_new_secret = SecretKey::from_bytes(rng.random_array()?);
        let our_new_public_key = our_new_secret.public_key()?;

        let their_new_secret = SecretKey::from_bytes(rng.random_array()?);
        let their_new_public_key = their_new_secret.public_key()?;

        Ok((
            NewKeysForUs {
                our_new_secret,
                their_new_public_key,
            },
            NewKeysForThem {
                our_new_public_key,
                their_new_secret,
            },
        ))
    }
}

#[derive(Debug, Error)]
pub enum TwoPartyError {
    #[error(transparent)]
    Hpke(#[from] HpkeError),

    #[error(transparent)]
    X3DH(#[from] X3DHError),

    #[error(transparent)]
    Rng(#[from] RngError),

    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error(transparent)]
    Decode(#[from] DecodeError),

    #[error(transparent)]
    X25519(#[from] X25519Error),

    #[error("prekeys have already been used")]
    PreKeyReuse,

    #[error("tried to decrypt with unknown 2SM secret at index {0}")]
    UnknownSecretUsed(u64),

    #[error("invalid ciphertext for message type")]
    InvalidCiphertextType,
}

#[cfg(test)]
mod tests {
    use crate::crypto::Rng;
    use crate::crypto::x25519::SecretKey;
    use crate::key_bundle::Lifetime;
    use crate::key_manager::KeyManager;
    use crate::traits::PreKeyManager;

    use super::OneTimeTwoParty;

    #[test]
    fn two_party_secret_messaging_protocol() {
        let rng = Rng::from_seed([1; 32]);

        // Alice generates their key material.

        let alice_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let alice_manager =
            KeyManager::init(&alice_identity_secret, Lifetime::default(), &rng).unwrap();

        let (alice_manager, alice_prekey_bundle) =
            KeyManager::generate_onetime_bundle(alice_manager, &rng).unwrap();

        // Bob generates their key material.

        let bob_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let bob_manager =
            KeyManager::init(&bob_identity_secret, Lifetime::default(), &rng).unwrap();

        let (bob_manager, bob_prekey_bundle) =
            KeyManager::generate_onetime_bundle(bob_manager, &rng).unwrap();

        // Alice and Bob set up the 2SM protocol handlers for each other.

        let alice_2sm = OneTimeTwoParty::init(bob_prekey_bundle);
        let bob_2sm = OneTimeTwoParty::init(alice_prekey_bundle);

        // They start exchanging "secret messages" to each other.

        let (alice_2sm, message_1) =
            OneTimeTwoParty::send(alice_2sm, &alice_manager, b"Hello, Bob!", &rng).unwrap();
        let (bob_2sm, bob_manager, receive_1) =
            OneTimeTwoParty::receive(bob_2sm, bob_manager, message_1).unwrap();

        let (alice_2sm, message_2) =
            OneTimeTwoParty::send(alice_2sm, &alice_manager, b"How are you doing?", &rng).unwrap();
        let (bob_2sm, bob_manager, receive_2) =
            OneTimeTwoParty::receive(bob_2sm, bob_manager, message_2).unwrap();

        let (bob_2sm, message_3) =
            OneTimeTwoParty::send(bob_2sm, &bob_manager, b"I'm alright. Thank you!", &rng).unwrap();
        let (alice_2sm, alice_manager, receive_3) =
            OneTimeTwoParty::receive(alice_2sm, alice_manager, message_3).unwrap();

        let (bob_2sm, message_4) =
            OneTimeTwoParty::send(bob_2sm, &bob_manager, b"How are you?", &rng).unwrap();
        let (alice_2sm, alice_manager, receive_4) =
            OneTimeTwoParty::receive(alice_2sm, alice_manager, message_4).unwrap();

        let (alice_2sm, message_5) =
            OneTimeTwoParty::send(alice_2sm, &alice_manager, b"I'm bored.", &rng).unwrap();
        let (bob_2sm, bob_manager, receive_5) =
            OneTimeTwoParty::receive(bob_2sm, bob_manager, message_5).unwrap();

        assert_eq!(receive_1, b"Hello, Bob!");
        assert_eq!(receive_2, b"How are you doing?");
        assert_eq!(receive_3, b"I'm alright. Thank you!");
        assert_eq!(receive_4, b"How are you?");
        assert_eq!(receive_5, b"I'm bored.");

        // They write a message to each other at the same time.

        let (bob_2sm, message_6) =
            OneTimeTwoParty::send(bob_2sm, &bob_manager, b":-(", &rng).unwrap();
        let (alice_2sm, message_7) =
            OneTimeTwoParty::send(alice_2sm, &alice_manager, b"Oh wait.", &rng).unwrap();

        let (_, _, receive_6) =
            OneTimeTwoParty::receive(alice_2sm, alice_manager, message_6).unwrap();
        let (_, _, receive_7) = OneTimeTwoParty::receive(bob_2sm, bob_manager, message_7).unwrap();

        assert_eq!(receive_6, b":-(");
        assert_eq!(receive_7, b"Oh wait.");
    }
}
