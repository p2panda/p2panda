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
use crate::two_party::{X3dhCiphertext, X3dhError, x3dh_decrypt, x3dh_encrypt};

/// Two-Party Secure Messaging (2SM) Key Agreement Protocol as specified in the paper "Key
/// Agreement for Decentralized Secure Group Messaging with Strong Security Guarantees" (2020).
///
/// 2SM is used for key-agreement as part of the DCGKA protocol allowing all members to learn about
/// the "seed" for establishing new secret state. 2SM is pair-wise between all members of an
/// encrypted group. p2panda uses 2SM for both "data-" and "message encryption" schemes.
///
/// ## Protocol
///
/// An initiator "Alice" of a 2SM session uses the pre-keys of "Bob" to send the first encrypted
/// message using the X3DH protocol. This only takes place once and the pre-keys can be considered
/// "used" afterwards (which is especially important for one-time pre-keys).
///
/// All subsequent messages sent between Alice and Bob are encrypted using the HPKE protocol. For
/// each round the sender uses the previous keys for HPKE and generates and attaches to the payload
/// a new key-pair for future rounds.
///
/// To avoid reusing public keys (which would make FS impossible), whenever a party sends a
/// message, it also updates the other party's public key. To do so, it sends a new secret key
/// along with its message, then deletes its own copy, storing only the public key.
///
/// To accommodate for messages arriving "late", the secret key is kept until it or a newer secret
/// has been used. In the case of a newer secret being used, all "previous" secret keys will be
/// dropped.
///
/// ## Forward secrecy
///
/// During the initial 2SM "round" using X3DH the forward secrecy is defined by the lifetime of the
/// used pre-keys. For strong security guarantees it is recommended to use one-time pre-keys. If
/// this requirement can be relaxed it is possible to use long-term pre-keys, with a lifetime
/// defined by the application.
///
/// Each subsequent 2SM HPKE round uses exactly one secret key, which is then dropped and replaced
/// by a newly-generated key-pair. This gives the key-agreement protocol strong forward secrecy
/// guarantees for each round, independent of the used pre-keys.
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
/// ## Message Ordering
///
/// 2SM assumes that all messages are received in the order they have been sent. The application or
/// underlying networking protocol needs to handle ordering. This is handled for us by the DCGKA
/// protocol (as specified in the paper) and causally-ordered, authenticated broadcast in p2panda
/// itself.
///
/// <https://eprint.iacr.org/2020/1281.pdf>
pub struct TwoParty<KMG, KB> {
    _marker: PhantomData<(KMG, KB)>,
}

/// 2SM protocol with one-time pre-keys.
pub type OneTimeTwoParty = TwoParty<KeyManager, OneTimeKeyBundle>;

/// 2SM protocol with long-term pre-keys (with a specified lifetime).
pub type LongTermTwoParty = TwoParty<KeyManager, LongTermKeyBundle>;

/// State of 2SM session between two members.
///
/// All 2SM methods are expressed as "pure functions" without any side-effects, returning an
/// updated state object. This allows applications to be more crash-resiliant, persisting the final
/// state only when all processes have successfully completed.
///
/// The state is serializable and can be used to persist 2SM sessions.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct TwoPartyState<KB: KeyBundle> {
    /// Index of key we will use during next send. The receiver can use the public key and refer to
    /// it through that index when they want to encrypt a message back to us.
    our_next_key_index: u64,

    /// Index of the last key which was used by the other peer to encrypt a message towards us. We
    /// keep it around to understand which secret keys we can remove.
    our_min_key_index: u64,

    /// List of all secret keys we generated ourselves. We sent the public counterpart to the other
    /// peer.
    our_secret_keys: HashMap<u64, SecretKey>,

    /// Last secret key the other peer generated for us. This is part of the 2SM protocol and an
    /// optimization where the remote end can _also_ generate secrets for us.
    our_received_secret_key: Option<SecretKey>,

    /// Which key we use to decrypt the next incoming message.
    their_next_key_used: KeyUsed,

    /// Public identity key of the other peer. We use it to verify the signature of their prekey.
    their_identity_key: PublicKey,

    /// Key-material we need to encrypt the first message with the help of X3DH and prekeys.
    their_prekey_bundle: Option<KB>,

    /// Last known public key of the other peer. We use it to encrypt a message towards them.
    their_public_key: Option<PublicKey>,
}

// Public methods.

impl<KMG, KB> TwoParty<KMG, KB>
where
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    KB: KeyBundle,
{
    /// Initialise new 2SM state using the other party's pre-key bundle.
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

    /// Securely send a `plaintext` message to the other party.
    pub fn send(
        y: TwoPartyState<KB>,
        y_manager: &KMG::State,
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

    /// Handle receiving a secure message from the other party.
    pub fn receive(
        y: TwoPartyState<KB>,
        y_manager: KMG::State,
        message: TwoPartyMessage,
    ) -> TwoPartyResult<(TwoPartyState<KB>, KMG::State, Vec<u8>)> {
        let (mut y_i, y_manager_i, plaintext_bytes) =
            Self::decrypt(y, y_manager, message.ciphertext, message.key_used)?;
        let plaintext_message = TwoPartyPlaintext::from_bytes(&plaintext_bytes)?;

        y_i.their_public_key = Some(plaintext_message.sender_new_public_key);
        y_i.their_next_key_used = KeyUsed::OwnKey(plaintext_message.sender_next_index);
        y_i.our_received_secret_key = Some(plaintext_message.receiver_new_secret);

        Ok((y_i, y_manager_i, plaintext_message.plaintext))
    }
}

/// 2SM states indicating which key material was used.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)]
pub enum KeyUsed {
    /// Previously published keys ("prekeys") for X3DH.
    PreKey,

    /// Key the receiving peer received last time from the sending peer for HPKE.
    ReceivedKey,

    /// Key the receiving peer generated themselves at some time. We can refer to the exact key by
    /// it's index.
    OwnKey(u64),
}

/// 2SM message to be sent over the network.
///
/// Note that this does not contain any additional information about the sender and receiver. This
/// information needs to be added in applications.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoPartyMessage {
    ciphertext: TwoPartyCiphertext,
    key_used: KeyUsed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TwoPartyCiphertext {
    /// Message was encrypted using X3DH pre-keys (initial round).
    PreKey(X3dhCiphertext),

    /// Message was encrypted using HPKE.
    Hpke(HpkeCiphertext),
}

/// Payload from sender which will be encrypted containing the actual message `plaintext` and
/// meta-data we need for the 2SM protocol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoPartyPlaintext {
    /// Secret message for the receiver.
    plaintext: Vec<u8>,

    /// Newly generated secret for the receiver, to be used in future 2SM rounds.
    receiver_new_secret: SecretKey,

    /// Newly generated public key of the sender, to be used in future 2SM rounds.
    sender_new_public_key: PublicKey,

    /// Index of the newly generated key of the sender, the receiver refers to it when using it in
    /// future 2SM rounds.
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

impl<KMG, KB> TwoParty<KMG, KB>
where
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    KB: KeyBundle,
{
    /// Encrypt a message toward the other party using X3DH when it is the first round or HPKE for
    /// subsequent rounds.
    fn encrypt(
        mut y: TwoPartyState<KB>,
        y_manager: &KMG::State,
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
                    KMG::identity_secret(y_manager),
                    &their_prekey_bundle,
                    rng,
                )?;
                TwoPartyCiphertext::PreKey(ciphertext)
            }
            Some(their_public_key) => {
                let ciphertext = hpke_seal(their_public_key, None, None, plaintext)?;
                TwoPartyCiphertext::Hpke(ciphertext)
            }
        };

        Ok((y, ciphertext))
    }

    /// Decrypt a message from the other party using X3DH when it is the first round or HPKE for
    /// subsequent rounds.
    fn decrypt(
        mut y: TwoPartyState<KB>,
        y_manager: KMG::State,
        ciphertext: TwoPartyCiphertext,
        key_used: KeyUsed,
    ) -> TwoPartyResult<(TwoPartyState<KB>, KMG::State, Vec<u8>)> {
        let (y_manager_i, plaintext) = match key_used {
            KeyUsed::PreKey => {
                let TwoPartyCiphertext::PreKey(ciphertext) = ciphertext else {
                    return Err(TwoPartyError::InvalidCiphertextType);
                };

                // If the underlying key manager provides a one-time secret, we use it here.
                let (y_manager_i, onetime_secret) = match ciphertext.onetime_prekey_id {
                    Some(onetime_prekey_id) => {
                        let (y_manager_i, onetime_secret) =
                            KMG::use_onetime_secret(y_manager, onetime_prekey_id)
                                .map_err(|_| TwoPartyError::PreKeyReuse)?;
                        (y_manager_i, onetime_secret)
                    }
                    None => (y_manager, None),
                };

                let plaintext = x3dh_decrypt(
                    &ciphertext,
                    KMG::identity_secret(&y_manager_i),
                    KMG::prekey_secret(&y_manager_i),
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

                for i in y.our_min_key_index..index + 1 {
                    y.our_secret_keys.remove(&i);
                }
                y.our_min_key_index = index + 1;

                (y_manager, plaintext)
            }
        };

        Ok((y, y_manager_i, plaintext))
    }
}

impl<KMG, KB> TwoParty<KMG, KB> {
    /// Generate fresh key material for us and the other party for future 2SM rounds.
    ///
    /// This material is sent as part of the encrypted ciphertext, attached next to the actual
    /// secret message. Each party prepares the received keys to be available for future rounds.
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

struct NewKeysForUs {
    our_new_secret: SecretKey,
    their_new_public_key: PublicKey,
}

struct NewKeysForThem {
    our_new_public_key: PublicKey,
    their_new_secret: SecretKey,
}

pub type TwoPartyResult<T> = Result<T, TwoPartyError>;

#[derive(Debug, Error)]
pub enum TwoPartyError {
    #[error(transparent)]
    Hpke(#[from] HpkeError),

    #[error(transparent)]
    X3dh(#[from] X3dhError),

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

    use super::{KeyUsed, LongTermTwoParty, OneTimeTwoParty, TwoPartyError};

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

        // Alice doesn't have any secret HPKE keys yet and will use Bob's pre-keys for X3DH.
        assert_eq!(alice_2sm.our_secret_keys.len(), 0);
        assert_eq!(alice_2sm.our_min_key_index, 1);
        assert_eq!(alice_2sm.our_next_key_index, 1);
        assert_eq!(alice_2sm.their_next_key_used, KeyUsed::PreKey);

        let bob_2sm = OneTimeTwoParty::init(alice_prekey_bundle);

        // Bob doesn't have any secret HPKE keys yet and will use Alice's pre-keys for X3DH.
        assert_eq!(bob_2sm.our_secret_keys.len(), 0);
        assert_eq!(bob_2sm.our_min_key_index, 1);
        assert_eq!(bob_2sm.our_next_key_index, 1);
        assert_eq!(bob_2sm.their_next_key_used, KeyUsed::PreKey);

        // They start exchanging "secret messages" to each other.

        // 1. Alice sends a message to Bob using their pre-keys for X3DH.
        let (alice_2sm, message_1) =
            OneTimeTwoParty::send(alice_2sm, &alice_manager, b"Hello, Bob!", &rng).unwrap();

        // Alice generated their own secret key and sends the public part to Bob for future rounds.
        assert_eq!(alice_2sm.our_secret_keys.len(), 1);
        assert_eq!(alice_2sm.our_min_key_index, 1);
        assert_eq!(alice_2sm.our_next_key_index, 2);

        // Alice also generated the secret key for Bob, so Alice also knows their public key
        // for the future already.
        assert!(alice_2sm.their_public_key.is_some());

        // Alice didn't receive anything from Bob yet.
        assert!(alice_2sm.our_received_secret_key.is_none());

        // In the future Alice would use the key they just generated to decrypt messages from Bob.
        assert_eq!(alice_2sm.their_next_key_used, KeyUsed::ReceivedKey);

        // Alice dropped the now-used pre-keys of Bob.
        assert!(alice_2sm.their_prekey_bundle.is_none());

        // 2. Bob receives Alice's message.
        let (bob_2sm, bob_manager, receive_1) =
            OneTimeTwoParty::receive(bob_2sm, bob_manager, message_1).unwrap();

        // Bob still doesn't have any generated keys yet.
        assert_eq!(bob_2sm.our_secret_keys.len(), 0);
        assert_eq!(bob_2sm.our_min_key_index, 1);
        assert_eq!(bob_2sm.our_next_key_index, 1);

        // Bob learned about the new public key (1) of Alice.
        assert_eq!(
            bob_2sm
                .their_public_key
                .expect("bob learned about public key of alice"),
            alice_2sm
                .our_secret_keys
                .get(&1)
                .expect("alice has one secret key")
                .public_key()
                .unwrap()
        );

        // Bob got their new secret key from Alice.
        assert!(bob_2sm.our_received_secret_key.is_some());

        // Bob would use Alice's new secret key for decrypting future messages.
        assert_eq!(bob_2sm.their_next_key_used, KeyUsed::OwnKey(1));

        // Bob still has Alice's pre-key bundle.
        assert!(bob_2sm.their_prekey_bundle.is_some());

        // 3. Alice sends another message to Bob and they receive it.
        let (alice_2sm, message_2) =
            OneTimeTwoParty::send(alice_2sm, &alice_manager, b"How are you doing?", &rng).unwrap();
        let (bob_2sm, bob_manager, receive_2) =
            OneTimeTwoParty::receive(bob_2sm, bob_manager, message_2).unwrap();

        // Alice generated another secret and keeps now two of them around as Bob didn't reply yet.
        assert_eq!(alice_2sm.our_secret_keys.len(), 2);
        assert_eq!(alice_2sm.our_min_key_index, 1);
        assert_eq!(alice_2sm.our_next_key_index, 3);

        // The secret keys are unique for each round.
        assert_ne!(
            alice_2sm.our_secret_keys.get(&1).unwrap(),
            alice_2sm.our_secret_keys.get(&2).unwrap(),
        );

        // Bob learned about the new public key (2) of Alice.
        assert_eq!(
            bob_2sm
                .their_public_key
                .expect("bob learned about public key of alice"),
            alice_2sm
                .our_secret_keys
                .get(&2)
                .expect("alice has one secret key")
                .public_key()
                .unwrap()
        );

        // 4. Bob answers to Alice.
        let (bob_2sm, message_3) =
            OneTimeTwoParty::send(bob_2sm, &bob_manager, b"I'm alright. Thank you!", &rng).unwrap();

        // Bob used Alice's latest public key (2) to encrypt this message.
        assert_eq!(message_3.key_used, KeyUsed::OwnKey(2));

        // Bob generated their own secret key and sends the public part to Alice for future rounds.
        assert_eq!(bob_2sm.our_secret_keys.len(), 1);
        assert_eq!(bob_2sm.our_min_key_index, 1);
        assert_eq!(bob_2sm.our_next_key_index, 2);

        // Bob assumes now that Alice will use Bob's secret key for future messages.
        assert_eq!(bob_2sm.their_next_key_used, KeyUsed::ReceivedKey);

        // 5. Alice receives the message from Bob.
        let (alice_2sm, alice_manager, receive_3) =
            OneTimeTwoParty::receive(alice_2sm, alice_manager, message_3).unwrap();

        // Alice removed the used secret key of this message (2) and all previous secrets as well
        // (1) for forward secrecy.
        assert_eq!(alice_2sm.our_secret_keys.len(), 0);
        assert_eq!(alice_2sm.our_min_key_index, 3);
        assert_eq!(alice_2sm.our_next_key_index, 3);

        // 6. Both parties continue chatting with each other ..
        let (bob_2sm, message_4) =
            OneTimeTwoParty::send(bob_2sm, &bob_manager, b"How are you?", &rng).unwrap();
        let (alice_2sm, alice_manager, receive_4) =
            OneTimeTwoParty::receive(alice_2sm, alice_manager, message_4).unwrap();

        let (alice_2sm, message_5) =
            OneTimeTwoParty::send(alice_2sm, &alice_manager, b"I'm bored.", &rng).unwrap();
        let (bob_2sm, bob_manager, receive_5) =
            OneTimeTwoParty::receive(bob_2sm, bob_manager, message_5).unwrap();

        // Messages can be correctly decrypted.
        assert_eq!(receive_1, b"Hello, Bob!");
        assert_eq!(receive_2, b"How are you doing?");
        assert_eq!(receive_3, b"I'm alright. Thank you!");
        assert_eq!(receive_4, b"How are you?");
        assert_eq!(receive_5, b"I'm bored.");

        // 7. They write a message to each other at the same time.
        let (bob_2sm, message_6) =
            OneTimeTwoParty::send(bob_2sm, &bob_manager, b":-(", &rng).unwrap();
        let (alice_2sm, message_7) =
            OneTimeTwoParty::send(alice_2sm, &alice_manager, b"Oh wait.", &rng).unwrap();

        let (alice_2sm, _, receive_6) =
            OneTimeTwoParty::receive(alice_2sm, alice_manager, message_6).unwrap();
        let (bob_2sm, _, receive_7) =
            OneTimeTwoParty::receive(bob_2sm, bob_manager, message_7).unwrap();

        assert_eq!(receive_6, b":-(");
        assert_eq!(receive_7, b"Oh wait.");

        // Both Alice and Bob still only have one secret key and all other keys have been removed.
        assert_eq!(alice_2sm.our_secret_keys.len(), 1);
        assert_eq!(bob_2sm.our_secret_keys.len(), 1);
    }

    #[test]
    fn long_term_prekeys() {
        let rng = Rng::from_seed([1; 32]);

        // Alice generates their long-term key material.

        let alice_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let alice_manager =
            KeyManager::init(&alice_identity_secret, Lifetime::default(), &rng).unwrap();

        let alice_prekey_bundle = KeyManager::prekey_bundle(&alice_manager);

        // Bob generates their long-term key material.

        let bob_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let bob_manager =
            KeyManager::init(&bob_identity_secret, Lifetime::default(), &rng).unwrap();

        let bob_prekey_bundle = KeyManager::prekey_bundle(&bob_manager);

        // Alice and Bob set up the 2SM protocol handlers for each other.

        let alice_2sm_a = LongTermTwoParty::init(bob_prekey_bundle.clone());
        let bob_2sm_a = LongTermTwoParty::init(alice_prekey_bundle.clone());

        // They start exchanging "secret messages" to each other in Group A.

        let (alice_2sm_a, message_1) =
            LongTermTwoParty::send(alice_2sm_a, &alice_manager, b"Hello, Bob!", &rng).unwrap();

        // Public key of "Bob" for the first round in Group A.
        let bob_public_key_1 = alice_2sm_a.their_public_key;

        let (bob_2sm_a, bob_manager, receive_1) =
            LongTermTwoParty::receive(bob_2sm_a, bob_manager, message_1).unwrap();

        let (_bob_2sm_a, message_2) =
            LongTermTwoParty::send(bob_2sm_a, &bob_manager, b"Hello, Alice!", &rng).unwrap();
        let (_alice_2sm_a, alice_manager, receive_2) =
            LongTermTwoParty::receive(alice_2sm_a, alice_manager, message_2).unwrap();

        assert_eq!(receive_1, b"Hello, Bob!");
        assert_eq!(receive_2, b"Hello, Alice!");

        // Sometime later they start another group B with the same long-term pre-keys.

        let alice_2sm_b = LongTermTwoParty::init(bob_prekey_bundle);
        let bob_2sm_b = LongTermTwoParty::init(alice_prekey_bundle);

        // They start exchanging "secret messages" to each other.

        let (alice_2sm_b, message_1) =
            LongTermTwoParty::send(alice_2sm_b, &alice_manager, b"Hello, again, Bob!", &rng)
                .unwrap();

        // Public key of "Bob" for the first round in Group B.
        let bob_public_key_2 = alice_2sm_b.their_public_key;

        let (bob_2sm_b, bob_manager, receive_1) =
            LongTermTwoParty::receive(bob_2sm_b, bob_manager, message_1).unwrap();

        let (_bob_2sm_b, message_2) =
            LongTermTwoParty::send(bob_2sm_b, &bob_manager, b"Hello, again, Alice!", &rng).unwrap();
        let (_alice_2sm_b, _alice_manager, receive_2) =
            LongTermTwoParty::receive(alice_2sm_b, alice_manager, message_2).unwrap();

        assert_eq!(receive_1, b"Hello, again, Bob!");
        assert_eq!(receive_2, b"Hello, again, Alice!");

        // The keys for the first round should be different across groups.

        assert_ne!(bob_public_key_1, bob_public_key_2);
    }

    #[test]
    fn invalid_replayed_messages() {
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

        // Alice sends a message to Bob.

        let (alice_2sm, message_1) =
            OneTimeTwoParty::send(alice_2sm, &alice_manager, b"Hello, Bob!", &rng).unwrap();
        let (bob_2sm, bob_manager, _receive_1) =
            OneTimeTwoParty::receive(bob_2sm, bob_manager, message_1.clone()).unwrap();

        // Bob receives the same message again.

        let result = OneTimeTwoParty::receive(bob_2sm.clone(), bob_manager.clone(), message_1);
        assert!(matches!(result, Err(TwoPartyError::PreKeyReuse)));

        // Alice sends another message to Bob.

        let (_alice_2sm, message_2) =
            OneTimeTwoParty::send(alice_2sm, &alice_manager, b"Hello, again, Bob!", &rng).unwrap();
        let (bob_2sm, bob_manager, _receive_2) =
            OneTimeTwoParty::receive(bob_2sm, bob_manager, message_2.clone()).unwrap();

        // Bob receives the same message again.

        let result = OneTimeTwoParty::receive(bob_2sm, bob_manager, message_2);
        assert!(matches!(result, Err(TwoPartyError::Hpke(_))));
    }
}
