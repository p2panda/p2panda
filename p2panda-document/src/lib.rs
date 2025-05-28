//! # Notes
//!
//! - Users can create a "universe"
//!     - A universe manages everything for that device
//!     - It also manages our key material (identity secret and pre keys for encryption)
//!     - This includes our regular p2panda private key for signing operations
//!     - It creates & signs operations automatically for groups (system), key bundles (system) and
//!     documents (system & application)
//!     - A universe has an "actor id", this is how other's can add us (our "device") to groups or
//!     documents
//! - Users can create "groups"
//!     - A group can be used to add members ("devices") or groups ("sub groups")
//! - Users can create "documents"
//!     - A document is a special group ("root group") which also manages encryption secrets
//!     - The user needs to be part of that document (in the beginning)
//!     - Application data can be encrypted inside a document and decrypted by other members
//!     - Documents use "data scheme" encryption
//!     - .. for "message scheme" we might want to introduce a new, separate thing
//! - Users publish key bundles
//!     - Universes manage the key material to generate bundles
//!     - Universes observe the network for published bundles and keep track of them in some sort
//!     of "address book" / key registry
//! - How should members be addressed in the user-facing API?
//!     - It's maybe too much to ask user's to know if it's an "individual" or "group"
//!     - Better if one just gives an ActorId / PublicKey and the internal system figures out if it
//!     belongs to a group or individual or is unknown
//!     - The unknown case is when we either don't have a key bundle / member message or a group
//!     message yet for that actor id
//!     - .. that should probably be an error case? For documents we will fail anyhow as we can't
//!     do much without a key bundle. For groups we might introduce bugs by addressing the actor
//!     wrongly?
//! - What events should the "universe" yield after calling `process` / `receive` on it with some
//! operations?
//!     - Learned about a new / updated member (observed key bundle)
//!     - Learned about a new / updated group
//!     - Learned about a new / updated document
//!     - We need to publish a new pre key bundle
//!     - We need to publish a new message (system- or application data)
//!     - Decrypted application data
//!     - Have we been invited somewhere to a document or group
//!     - Have we been removed somewhere, from a document or group
//!     - ...
//! - Ideally the generated events should not all be in memory but we can pick them up after each
//! other with an "stream consumer"
//! - "Members" are the "leaves" of the group graph
//!     - They can be addressed with an ActorId in the API
//!     - We _can't_ add them to a group / document if we don't have a key bundle of them yet
//!     - When adding a member to a group via it's actor id and the key bundle is expired /
//!     inexistant we throw an error
//!     - We need a way to calculate the "members", recursively when looking at a group, this is to
//!     determine the encryption `initial_members` etc.
//! - Universes observe the network for key bundles and automatically keep track of an key registry
//!     - We should allow implementations where this registry can be searched / filtered / etc.?
//!     - Maybe interesting for "address book" UIs
//! - There should be a way to manually export and import key bundles for a member
//!     - For example for cases where it got imported via scanning a QR code etc.
//!     - .. not using p2panda
//! - How are my own keys managed?
//!     - A universe is basically one single device / identity / member. We shouldn't offer
//!     managing multiple keys / identities within a universe, it would get too complicated too
//!     fast for now (how to communicate to the user how to manage multiple identities etc.)
//!     - When a universe gets created a new identity secret gets generated, the public identity
//!     part (for encryption) is connected now to the regular public key (for signing operations /
//!     identity handle)
//!     - Pre-keys can be rotated at any time
//!     - Some mechanism should exist which automatically generates new pre keys when the previous
//!     one expired
//!     - Users should be able to define the lifetime for their pre keys
//! - Created groups do not need to include ourselves
//! - Created documents need to include at least ourselves
//! - The universe object should implement the `WriteToStore` trait, inside of that trait
//! implementation we call the nested, other state objects, who also implement the same trait, like
//! this we can make sure that the whole "state tree" gets written into the database, into
//! different tables, all within one atomic transaction
//!     - Users should not need to worry too much about state handling
//!     - They only need to create a transaction object, write the universe state with it and
//!     commit the transaction
//!         - After receiving one or many operations and processing them
//!         - After calling one or many methods (like "create document" or "add member to group" or
//!         "rotate pre key", etc.)
//!     - The universe will take a store object which needs to implement a whole bunch of
//!     "read-only" store traits (for reading all sorts of states)
//! - For now we can have a lot of state in memory
//!     - Later we want to reduce as much memory use as possible, when it is not necessary to have
//!     it around all the time
//!     - Re-computing groups might not happen too often, for this it would be good to only load
//!     required state into memory when we need to change something
//!     - For access checks / authorization though we might need the state all the time, maybe we
//!     can keep that part in memory (not the whole tree, just the merged state CRDT result?)
//!     - For encryption we probably need to keep everything in memory as well, as applications
//!     create and read data all the time
//! - The orderer could live outside the universe and is rather part of the p2panda pipeline (with
//! validation first, then orderer, then the universe, etc.)
//! - Operations will have a bit of meta-data in the header extensions (like group id etc.)
//!     - How do we require / crate these extensions while allowing adding more as well (for
//!     application layer)
//! - The universe should detect if somebody used an outdated pre key of us and we should reject it
//!     - This is easy to do, we just need to regularily remove expired pre keys from our key
//!     manager
//! - How do we deal with document messages who arrive earlier than the key bundles?
//!     - Processing them will fail (as we can't decrypt the X3DH ciphertext from the first 2SM round)
//!     - Messages do not point at key bundles, that wouldn't anyway make sense as they can expire,
//!     or in "message scheme" they are even one-time only
//!     - We need another way to re-try as soon as that key bundle arrives
//!         - Probably need to establish a "waiting room" for failed document messages. We have a
//!         concrete error type when this happens (MissingPreKeys)
#![allow(unused)]
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::fmt::{self, Display};
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use p2panda_auth::group::{
    Access, Group as AuthGroupGeneric, GroupAction,
    GroupControlMessage as AuthControlMessageGeneric, GroupError as AuthGroupErrorGeneric,
    GroupMember, GroupResolver, GroupState as AuthGroupStateGeneric,
};
use p2panda_auth::traits::{
    AuthGroup as AuthGroupTrait, GroupStore, IdentityHandle as AuthIdentityHandle,
    Operation as AuthMessage, OperationId as AuthOperationId, Ordering as AuthOrdering,
};
use p2panda_core::{Hash, PrivateKey, PublicKey};
use p2panda_encryption::crypto::{SecretKey, XAeadNonce};
use p2panda_encryption::data_scheme::{
    ControlMessage as EncryptionControlMessage, DirectMessage as EncryptionDirectMessageGeneric,
    EncryptionGroup, EncryptionGroupError as EncryptionGroupErrorGeneric, GroupSecretId,
    GroupState as EncryptionGroupStateGeneric,
};
use p2panda_encryption::traits::{
    GroupMembership, GroupMessage as EncryptionMessage, GroupMessageType,
    IdentityHandle as EncryptionIdentityHandle, IdentityManager, IdentityRegistry, KeyBundle,
    OperationId as EncryptionOperationId, Ordering as EncryptionOrdering, PreKeyManager,
    PreKeyRegistry,
};
use p2panda_encryption::{
    KeyBundleError, KeyManager as KeyManagerInner, KeyManagerError,
    KeyManagerState as KeyManagerStateInner, KeyRegistry as KeyRegistryInner,
    KeyRegistryState as KeyRegistryStateInner, Lifetime, LongTermKeyBundle, Rng, RngError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;

// ~~~~~~~~~~
// Core types
// ~~~~~~~~~~

// Can be both a group id or individual id.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, StdHash, Serialize, Deserialize)]
pub struct ActorId(pub PublicKey);

impl Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AuthIdentityHandle for ActorId {}

impl EncryptionIdentityHandle for ActorId {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, StdHash)]
pub struct OperationId(pub Hash);

impl Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AuthOperationId for OperationId {}

impl EncryptionOperationId for OperationId {}

// ~~~~~~~~~~~~~~~~~~~
// Key manager wrapper
// ~~~~~~~~~~~~~~~~~~~

// Variant of `KeyManager` which can be shared across threads.
#[derive(Clone, Debug)]
pub struct KeyManager;

impl KeyManager {
    pub fn init(
        identity_secret: &SecretKey,
        lifetime: Lifetime,
        rng: &Rng,
    ) -> Result<KeyManagerState, KeyManagerError> {
        let inner = KeyManagerInner::init(identity_secret, lifetime, rng)?;
        Ok(KeyManagerState {
            inner: Arc::new(Mutex::new(Some(inner))),
        })
    }
}

#[derive(Clone, Debug)]
pub struct KeyManagerState {
    inner: Arc<Mutex<Option<KeyManagerStateInner>>>,
}

impl IdentityManager<KeyManagerState> for KeyManager {
    fn identity_secret(y: &KeyManagerState) -> &SecretKey {
        // TODO: Can't return a mutex guard here.
        todo!()
    }
}

impl PreKeyManager for KeyManager {
    type State = KeyManagerState;

    type Error = KeyManagerError;

    fn prekey_secret(y: &Self::State) -> &SecretKey {
        // TODO: Can't return a mutex guard here.
        todo!()
    }

    fn rotate_prekey(
        y: Self::State,
        lifetime: Lifetime,
        rng: &Rng,
    ) -> Result<Self::State, Self::Error> {
        let mut inner = y.inner.lock().unwrap();
        let y_inner = inner.take().expect("inner state");
        let y_inner_ii = KeyManagerInner::rotate_prekey(y_inner, lifetime, rng)?;
        *inner = Some(y_inner_ii);
        drop(inner);
        Ok(y)
    }

    fn prekey_bundle(y: &Self::State) -> LongTermKeyBundle {
        let inner = y.inner.lock().unwrap();
        KeyManagerInner::prekey_bundle(inner.as_ref().expect("inner state"))
    }

    fn generate_onetime_bundle(
        y: Self::State,
        rng: &Rng,
    ) -> Result<(Self::State, p2panda_encryption::OneTimeKeyBundle), Self::Error> {
        unreachable!("no onetime pre-keys used in data encryption scheme")
    }

    fn use_onetime_secret(
        y: Self::State,
        id: p2panda_encryption::OneTimePreKeyId,
    ) -> Result<(Self::State, Option<SecretKey>), Self::Error> {
        unreachable!("no onetime pre-keys used in data encryption scheme")
    }
}

impl Serialize for KeyManagerState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let inner = self.inner.lock().unwrap();
        inner.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for KeyManagerState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let inner = KeyManagerStateInner::deserialize(deserializer)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(Some(inner))),
        })
    }
}

// ~~~~~~~~~~~~~~~~~~~~
// Key registry wrapper
// ~~~~~~~~~~~~~~~~~~~~

// Variant of `KeyRegistry` which can be shared across threads.
#[derive(Clone, Debug)]
pub struct KeyRegistry;

impl KeyRegistry {
    pub fn init() -> KeyRegistryState {
        KeyRegistryState {
            inner: Arc::new(Mutex::new(Some(KeyRegistryInner::init()))),
        }
    }

    pub fn register_key_bundle(
        mut y: KeyRegistryState,
        id: ActorId,
        key_bundle: LongTermKeyBundle,
    ) -> KeyRegistryState {
        let mut inner = y.inner.lock().unwrap();
        let y_inner = inner.take().expect("inner key registry state to be given");
        let y_inner_ii = KeyRegistryInner::add_longterm_bundle(y_inner, id, key_bundle);
        *inner = Some(y_inner_ii);
        drop(inner);
        y
    }
}

#[derive(Clone, Debug)]
pub struct KeyRegistryState {
    inner: Arc<Mutex<Option<KeyRegistryStateInner<ActorId>>>>,
}

impl PreKeyRegistry<ActorId, LongTermKeyBundle> for KeyRegistry {
    type State = KeyRegistryState;

    type Error = Infallible;

    fn key_bundle(
        y: Self::State,
        id: &ActorId,
    ) -> Result<(Self::State, Option<LongTermKeyBundle>), Self::Error> {
        let mut inner = y.inner.lock().unwrap();
        let y_inner = inner.take().expect("inner key registry state to be given");
        let Ok((y_inner_ii, bundle)) = KeyRegistryInner::key_bundle(y_inner, id);
        *inner = Some(y_inner_ii);
        drop(inner);
        Ok((y, bundle))
    }
}

impl IdentityRegistry<ActorId, KeyRegistryState> for KeyRegistry {
    type Error = Infallible;

    fn identity_key(
        y: &KeyRegistryState,
        id: &ActorId,
    ) -> Result<Option<p2panda_encryption::crypto::PublicKey>, Self::Error> {
        let inner = y.inner.lock().unwrap();
        let y_inner = inner
            .as_ref()
            .expect("inner key registry state to be given");
        let Ok(result) = KeyRegistryInner::identity_key(y_inner, id);
        Ok(result)
    }
}

impl Serialize for KeyRegistryState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let inner = self.inner.lock().unwrap();
        inner.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for KeyRegistryState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let inner = KeyRegistryStateInner::deserialize(deserializer)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(Some(inner))),
        })
    }
}

// ~~~~~~~~~~~~~~~~
// Auth group types
// ~~~~~~~~~~~~~~~~

pub type AuthResolver<C, GS> = GroupResolver<ActorId, OperationId, C, Orderer<C, GS>, GS>;

pub type AuthGroup<C, GS> =
    AuthGroupGeneric<ActorId, OperationId, C, AuthResolver<C, GS>, Orderer<C, GS>, GS>;

pub type AuthGroupState<C, GS> =
    AuthGroupStateGeneric<ActorId, OperationId, C, AuthResolver<C, GS>, Orderer<C, GS>, GS>;

pub type AuthControlMessage<C> = AuthControlMessageGeneric<ActorId, OperationId, C>;

pub type AuthGroupError<C, GS> =
    AuthGroupErrorGeneric<ActorId, OperationId, C, AuthResolver<C, GS>, Orderer<C, GS>, GS>;

pub type EncryptionGroupState<C, GS> = EncryptionGroupStateGeneric<
    ActorId,
    OperationId,
    KeyRegistry,
    EncryptionGroupManager,
    KeyManager,
    Orderer<C, GS>,
>;

pub type EncryptionGroupError<C, GS> = EncryptionGroupErrorGeneric<
    ActorId,
    OperationId,
    KeyRegistry,
    EncryptionGroupManager,
    KeyManager,
    Orderer<C, GS>,
>;

pub type EncryptionDirectMessage =
    EncryptionDirectMessageGeneric<ActorId, OperationId, EncryptionGroupManager>;

// ~~~~~~~~~~~~~~
// Encryption DGM
// ~~~~~~~~~~~~~~

#[derive(Clone, Debug)]
pub struct EncryptionGroupManager;

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct EncryptionGroupManagerState;

impl GroupMembership<ActorId, OperationId> for EncryptionGroupManager {
    type State = EncryptionGroupManagerState;

    type Error = Infallible;

    fn create(_my_id: ActorId, _initial_members: &[ActorId]) -> Result<Self::State, Self::Error> {
        Ok(EncryptionGroupManagerState::default())
    }

    fn from_welcome(_my_id: ActorId, y: Self::State) -> Result<Self::State, Self::Error> {
        Ok(y)
    }

    fn add(
        y: Self::State,
        _adder: ActorId,
        _added: ActorId,
        _operation_id: OperationId,
    ) -> Result<Self::State, Self::Error> {
        Ok(y)
    }

    fn remove(
        y: Self::State,
        _remover: ActorId,
        _removed: &ActorId,
        _operation_id: OperationId,
    ) -> Result<Self::State, Self::Error> {
        Ok(y)
    }

    fn members(_y: &Self::State) -> Result<HashSet<ActorId>, Self::Error> {
        todo!()
    }
}

// ~~~~~~~
// Orderer
// ~~~~~~~

#[derive(Clone, Debug)]
pub struct Orderer<C, GS> {
    _marker: PhantomData<(C, GS)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrdererState {
    my_id: ActorId,
}

impl<C, GS> AuthOrdering<ActorId, OperationId, AuthControlMessage<C>> for Orderer<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    type State = OrdererState;

    type Message = Message<C>;

    type Error = DocumentError<C, GS>;

    fn next_message(
        y: Self::State,
        control_message: &AuthControlMessage<C>,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let sender = y.my_id;
        Ok((
            y,
            Message::PreAuth {
                sender,
                actor_id: control_message.group_id(),
                control_message: control_message.to_owned(),
            },
        ))
    }

    fn queue(y: Self::State, _message: &Self::Message) -> Result<Self::State, Self::Error> {
        Ok(y)
    }

    fn next_ready_message(
        y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        Ok((y, None))
    }
}

impl<C, GS> EncryptionOrdering<ActorId, OperationId, EncryptionGroupManager> for Orderer<C, GS> {
    type State = OrdererState;

    type Error = Infallible;

    type Message = Message<C>;

    fn next_control_message(
        y: Self::State,
        control_message: &EncryptionControlMessage<ActorId>,
        direct_messages: &[EncryptionDirectMessage],
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let sender = y.my_id;
        Ok((
            y,
            Message::PreEncryption {
                sender,
                control_message: control_message.clone(),
                direct_messages: direct_messages.to_vec(),
            },
        ))
    }

    fn next_application_message(
        _y: Self::State,
        _group_secret_id: GroupSecretId,
        _nonce: XAeadNonce,
        _ciphertext: Vec<u8>,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        todo!()
    }

    fn queue(_y: Self::State, _message: &Self::Message) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn set_welcome(y: Self::State, _message: &Self::Message) -> Result<Self::State, Self::Error> {
        // TODO: Noop?
        Ok(y)
    }

    fn next_ready_message(
        _y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        todo!()
    }
}

// ~~~~~~~~~~~~~~
// Fake Operation
// ~~~~~~~~~~~~~~

#[derive(Clone, Debug)]
pub struct FakeHeader {
    public_key: PublicKey,
    extensions: DocumentExtensions,
}

// TODO: Should we only use trait interfaces to be generic over the actual data type?
#[derive(Clone, Debug)]
pub struct FakeOperation<C> {
    header: FakeHeader,
    body: DocumentBody<C>,
    hash: Hash,
}

#[derive(Clone, Debug)]
pub enum GroupControlMessage<C> {
    Create {
        initial_members: Vec<(GroupMember<ActorId>, Access<C>)>,
    },
}

#[derive(Clone, Debug)]
pub enum DocumentControlMessage<C> {
    Create {
        initial_members: Vec<(GroupMember<ActorId>, Access<C>)>,
    },
}

// TODO: We can't trust that the set group member type (individual or group) is correct, this needs
// to be checked when we receive a message.
#[derive(Clone, Debug)]
pub enum DocumentBody<C> {
    Member {
        key_bundle: LongTermKeyBundle,
    },
    Group {
        control_message: GroupControlMessage<C>,
    },
    Document {
        control_message: DocumentControlMessage<C>,
        direct_messages: Vec<EncryptionDirectMessage>,
    },
}

#[derive(Clone, Debug)]
pub struct DocumentExtensions {
    version: u64,

    // TODO: What are the semantics here?
    actor_id: ActorId,
}

// ~~~~~~~
// Message
// ~~~~~~~

#[derive(Clone, Debug)]
pub enum Message<C> {
    PreAuth {
        sender: ActorId,
        actor_id: ActorId,
        control_message: AuthControlMessage<C>,
    },
    PreEncryption {
        sender: ActorId,
        control_message: EncryptionControlMessage<ActorId>,
        direct_messages: Vec<EncryptionDirectMessage>,
    },
    Signed(FakeOperation<C>),
}

impl<C> Message<C> {
    pub fn operation(self) -> Option<FakeOperation<C>> {
        match self {
            Message::PreAuth { .. } => None,
            Message::PreEncryption { .. } => None,
            Message::Signed(operation) => Some(operation),
        }
    }
}

impl<C> AuthMessage<ActorId, OperationId, AuthControlMessage<C>> for Message<C>
where
    C: Clone,
{
    fn id(&self) -> OperationId {
        match self {
            Message::Signed(operation) => OperationId(operation.hash),
            _ => unreachable!(),
        }
    }

    fn sender(&self) -> ActorId {
        match self {
            Message::PreAuth { sender, .. } => *sender,
            Message::PreEncryption { .. } => unreachable!(),
            Message::Signed(operation) => ActorId(operation.header.public_key),
        }
    }

    fn dependencies(&self) -> Vec<OperationId> {
        vec![]
    }

    fn previous(&self) -> Vec<OperationId> {
        vec![]
    }

    fn payload(&self) -> AuthControlMessage<C> {
        let message = match self {
            Message::Signed(operation) => match operation.body {
                DocumentBody::Group {
                    ref control_message,
                } => match control_message {
                    GroupControlMessage::Create { initial_members } => {
                        AuthControlMessage::GroupAction {
                            group_id: operation.header.extensions.actor_id,
                            action: GroupAction::Create {
                                initial_members: initial_members.to_vec(),
                            },
                        }
                    }
                },
                DocumentBody::Document {
                    ref control_message,
                    ..
                } => match control_message {
                    DocumentControlMessage::Create { initial_members } => {
                        AuthControlMessage::GroupAction {
                            group_id: operation.header.extensions.actor_id,
                            action: GroupAction::Create {
                                initial_members: initial_members.to_vec(),
                            },
                        }
                    }
                },
                _ => unreachable!(),
            },
            _ => unreachable!(),
        };
        message
    }
}

impl<C> EncryptionMessage<ActorId, OperationId, EncryptionGroupManager> for Message<C> {
    fn id(&self) -> OperationId {
        match self {
            Message::PreAuth { .. } => unreachable!(),
            Message::PreEncryption { .. } => OperationId(Hash::new(b"pre")),
            Message::Signed(operation) => OperationId(operation.hash),
        }
    }

    fn sender(&self) -> ActorId {
        match self {
            Message::PreAuth { .. } => unreachable!(),
            Message::PreEncryption { sender, .. } => *sender,
            Message::Signed(operation) => ActorId(operation.header.public_key),
        }
    }

    fn message_type(&self) -> GroupMessageType<ActorId> {
        todo!()
    }

    fn direct_messages(&self) -> Vec<EncryptionDirectMessage> {
        todo!()
    }
}

// ~~~~~
// Group
// ~~~~~

pub struct Group<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    id: ActorId,
    universe: Universe<C, GS>,
    _marker: PhantomData<C>,
}

impl<C, GS> Group<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    pub fn id(&self) -> ActorId {
        self.id
    }

    pub(crate) async fn create(
        universe_owned: Universe<C, GS>,
        initial_members: Vec<(GroupMember<ActorId>, Access<C>)>,
    ) -> Result<(Group<C, GS>, AuthGroupState<C, GS>, FakeOperation<C>), GroupError> {
        let universe = universe_owned.inner.read().await;

        // TODO: Here something happens with deriving a group id.
        let group_id = ActorId(PrivateKey::new().public_key());

        let y = AuthGroupState::new(
            universe.my_id,
            group_id,
            universe.store.clone(),
            universe.orderer.clone(),
        );

        let control_message = AuthControlMessage::GroupAction {
            group_id: group_id,
            action: GroupAction::Create {
                initial_members: initial_members.to_vec(),
            },
        };

        // TODO: We can't handle the error yet
        let (y_i, pre) = AuthGroup::prepare(y, &control_message).unwrap();

        // TODO: Use real p2panda operations with extensions here & sign them.
        let message = {
            Message::Signed(FakeOperation {
                header: FakeHeader {
                    public_key: universe.my_id.0,
                    extensions: DocumentExtensions {
                        version: 1,
                        actor_id: group_id,
                    },
                },
                body: DocumentBody::Group {
                    control_message: GroupControlMessage::Create {
                        initial_members: initial_members.to_vec(),
                    },
                },
                hash: Hash::from_bytes(universe.rng.random_array()?),
            })
        };

        // TODO: We can't handle the error yet
        let y_ii = AuthGroup::process(y_i, &message).unwrap();

        drop(universe);

        Ok((
            Group {
                id: group_id,
                universe: universe_owned,
                _marker: PhantomData,
            },
            y_ii,
            message
                .operation()
                .expect("operation should exist at this stage"),
        ))
    }
}

// ~~~~~~~~
// Document
// ~~~~~~~~

// This is merely a "pointer" at a document through holding it's id. On top this struct also has
// access to the "universe". This allows us to build a nice API where the users can handle objects
// like groups or documents independently while we internally handle all state inside the
// "universe". Finally the user can "commit" the changed state of the universe to the database.
pub struct Document<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    id: ActorId,
    universe: Universe<C, GS>,
    _marker: PhantomData<C>,
}

pub struct DocumentState<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    auth_state: AuthGroupState<C, GS>,
    encryption_state: EncryptionGroupState<C, GS>,
}

impl<C, GS> Document<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    pub(crate) async fn create(
        universe_owned: Universe<C, GS>,
        initial_members: Vec<(GroupMember<ActorId>, Access<C>)>,
    ) -> Result<(Document<C, GS>, DocumentState<C, GS>, FakeOperation<C>), DocumentError<C, GS>>
    {
        let universe = universe_owned.inner.read().await;

        // TODO: Here something happens with deriving a group id.
        let document_id = ActorId(PrivateKey::new().public_key());

        let (auth_state, auth_pre_message) = {
            let y = AuthGroupState::new(
                universe.my_id,
                document_id,
                universe.store.clone(),
                universe.orderer.clone(),
            );

            let control_message = AuthControlMessage::GroupAction {
                group_id: document_id,
                action: GroupAction::Create {
                    initial_members: initial_members.to_vec(),
                },
            };

            // TODO: We can't handle the error yet (see `DocumentError`).
            let (y_i, pre) = AuthGroup::prepare(y, &control_message).unwrap();

            (y_i, pre)
        };

        let (encryption_state, encryption_pre_message) = {
            // Every document gets their own key manager, the identity secret is the same (cloned)
            // but the pre-key will be different across documents.
            let y = EncryptionGroup::init(
                universe.my_id,
                universe.my_keys.clone(), // TODO: Make key manager RCed
                universe.pki.clone(),
                universe.dgm.clone(),     // TODO: Make DGM RCed?
                universe.orderer.clone(), // TODO: Make orderer RCed
            );

            // Compute set of members who are part of the encryption group.
            let initial_members = secret_members(
                // TODO: Can't handle error right now.
                auth_state
                    .transitive_members()
                    .expect("not sure how to handle this error"),
            );

            // TODO: Check if all pre keys for these initial members are not expired & given
            // (otherwise calling "create" will error).

            let (y_ii, pre) = EncryptionGroup::create(y, initial_members, &universe.rng)?;

            (y_ii, pre)
        };

        let Message::PreAuth { actor_id, .. } = auth_pre_message else {
            unreachable!("method will always return a pre-auth message");
        };

        let Message::PreEncryption {
            direct_messages, ..
        } = encryption_pre_message
        else {
            unreachable!("method will always return a pre-encryption message");
        };

        // TODO: Use real p2panda operations with extensions here & sign them.
        let message = {
            Message::Signed(FakeOperation {
                header: FakeHeader {
                    public_key: universe.my_id.0,
                    extensions: DocumentExtensions {
                        version: 1,
                        actor_id,
                    },
                },
                body: DocumentBody::Document {
                    control_message: DocumentControlMessage::Create { initial_members },
                    direct_messages,
                },
                hash: Hash::from_bytes(universe.rng.random_array()?),
            })
        };

        let auth_state = {
            let y_ii = AuthGroup::process(auth_state, &message).unwrap();
            y_ii
        };

        // TODO
        // We don't process encryption state here as this happened inside the method already. We
        // don't ack our own messages, on top the DGM is anyhow noop, so we can accept sending
        // "pre" messages in the encryption process flow (TBC?).

        drop(universe);

        Ok((
            Document {
                id: document_id,
                universe: universe_owned,
                _marker: PhantomData,
            },
            DocumentState {
                auth_state,
                encryption_state,
            },
            message
                .operation()
                .expect("operation should exist at this stage"),
        ))
    }

    // We call this after receiving a CREATE or ADD which brings us into a document.
    pub(crate) fn from_welcome(
        &self,
        _group_id: ActorId,
        _orderer: OrdererState,
    ) -> Result<AuthGroupState<C, GS>, DocumentError<C, GS>> {
        todo!()
    }

    pub fn id(&self) -> ActorId {
        self.id
    }

    pub async fn add(
        &self,
        added: GroupMember<ActorId>,
        access: Access<C>,
    ) -> Result<(), DocumentError<C, GS>> {
        // TODO: Basic checks here? Is this member already part of the group, do we try to add
        // ourselves, etc.?

        let mut universe = self.universe.inner.write().await;
        let mut y_doc = universe
            .documents
            .remove(&self.id)
            .ok_or(DocumentError::UnknownDocument(self.id))?;

        let auth_state = {
            let y = y_doc.auth_state;

            let control_message = AuthControlMessage::GroupAction {
                group_id: self.id,
                action: GroupAction::Add {
                    member: added,
                    access,
                },
            };

            // TODO: Prepare should not queue the operation for us (we don't need it inside the
            // orderer).
            // TODO: We can't handle the error yet (see `DocumentError`).
            let (y_i, operation) = AuthGroup::prepare(y, &control_message).unwrap();
            let y_ii = AuthGroup::process(y_i, &operation).unwrap();
            y_ii
        };

        y_doc.auth_state = auth_state;

        universe.documents.insert(self.id, y_doc);

        Ok(())
    }
}

// ~~~~~~~~
// Universe
// ~~~~~~~~

pub struct UniverseConfig {
    // This is when a key bundle gets considered expired and thus invalid.
    pre_key_lifetime: Duration,

    // We rotate our own pre keys after this duration, to allow some time between peers receiving
    // our new one and the old one expiring.
    pre_key_rotate_after: Duration,
}

impl Default for UniverseConfig {
    fn default() -> Self {
        Self {
            pre_key_lifetime: Duration::from_secs(60 * 60 * 24 * 90), // 90 days
            pre_key_rotate_after: Duration::from_secs(60 * 60 * 24 * 60), // 60 days
        }
    }
}

impl UniverseConfig {
    pub fn lifetime(&self) -> Lifetime {
        Lifetime::new(self.pre_key_lifetime.as_secs())
    }
}

// Messages we can see on the network inside a "universe" (app scope usually).
pub enum UniverseMessage {
    KeyBundle,
    Group,
    Document,
}

pub struct InnerUniverse<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    pub(crate) my_id: ActorId,

    pub(crate) config: UniverseConfig,

    pub(crate) my_keys: KeyManagerState,

    pub(crate) my_keys_rotated_at: u64, // UNIX timestamp in secs

    // Here we have all "leaves" aka, actual "devices" owning private keys.
    //
    // If we observed an individual we also have their key bundle. It could be that it expired some
    // time later though and that we need a new one after a while.
    pub(crate) individuals: HashSet<ActorId>,

    // Here we have _all_ groups EXCEPT "root groups" / documents.
    pub(crate) groups: HashMap<ActorId, AuthGroupState<C, GS>>,

    // Here we have all "root groups" / documents, no "sub groups".
    pub(crate) documents: HashMap<ActorId, DocumentState<C, GS>>,

    // Key bundles.
    pub(crate) pki: KeyRegistryState,

    pub(crate) store: GS,

    pub(crate) orderer: OrdererState,

    pub(crate) dgm: EncryptionGroupManagerState,

    pub(crate) rng: Rng,
}

// "App Universe", that's the "orchestrator" managing multiple documents and groups.
pub struct Universe<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    pub(crate) inner: Arc<RwLock<InnerUniverse<C, GS>>>,
}

impl<C, GS> Universe<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    pub fn new(
        my_id: ActorId,
        config: UniverseConfig,
        store: GS,
        rng: Rng,
    ) -> Result<Self, UniverseError<C, GS>> {
        // Generate pre keys with configured lifetime.
        let my_keys = {
            let identity_secret = SecretKey::from_rng(&rng)?;
            KeyManager::init(&identity_secret, config.lifetime(), &rng)?
        };
        let my_keys_rotated_at = now();

        // Register our own pre keys.
        let pki = {
            let key_bundle = KeyManager::prekey_bundle(&my_keys);
            let y = KeyRegistry::init();
            KeyRegistry::register_key_bundle(y, my_id, key_bundle)
        };

        // Add ourselves to "individuals" address book.
        let individuals = HashSet::from_iter([my_id]);

        // Establish initial states.
        let dgm = EncryptionGroupManagerState::default();
        let orderer = OrdererState { my_id };

        Ok(Self {
            inner: Arc::new(RwLock::new(InnerUniverse {
                my_id,
                config,
                my_keys,
                my_keys_rotated_at,
                individuals,
                groups: HashMap::new(),
                documents: HashMap::new(),
                pki,
                store,
                orderer,
                dgm,
                rng,
            })),
        })
    }

    pub async fn id(&self) -> ActorId {
        let inner = self.inner.read().await;
        inner.my_id
    }

    pub async fn create_document(
        &mut self,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<(Document<C, GS>, FakeOperation<C>), UniverseError<C, GS>> {
        let initial_members = self.identify_actor_types(initial_members).await?;

        let (document, y_doc, operation) = Document::create(self.clone(), initial_members).await?;

        let mut inner = self.inner.write().await;
        inner.documents.insert(document.id(), y_doc);

        Ok((document, operation))
    }

    pub async fn create_group(
        &mut self,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<(Group<C, GS>, FakeOperation<C>), UniverseError<C, GS>> {
        let initial_members = self.identify_actor_types(initial_members).await?;

        let (group, y_group, operation) = Group::create(self.clone(), initial_members).await?;

        let mut inner = self.inner.write().await;
        inner.groups.insert(group.id(), y_group);

        Ok((group, operation))
    }

    pub async fn key_bundle_expired(&self) -> bool {
        let inner = self.inner.read().await;
        now() - inner.my_keys_rotated_at > inner.config.pre_key_rotate_after.as_secs()
    }

    pub async fn key_bundle(&mut self) -> Result<FakeOperation<C>, UniverseError<C, GS>> {
        let mut inner = self.inner.write().await;

        // Automatically rotate pre key when it reached critical expiry date.
        if now() - inner.my_keys_rotated_at > inner.config.pre_key_rotate_after.as_secs() {
            inner.my_keys_rotated_at = now();
            // This mutates the state internally.
            KeyManager::rotate_prekey(inner.my_keys.clone(), inner.config.lifetime(), &inner.rng)?;
        }

        let key_bundle = KeyManager::prekey_bundle(&inner.my_keys);

        // Register our own key bundle.
        inner.pki =
            KeyRegistry::register_key_bundle(inner.pki.clone(), inner.my_id, key_bundle.clone());

        // TODO: Properly create and sign operations here.
        // TODO: Should this be a trait interface for signing and creating operations?
        Ok(FakeOperation {
            header: FakeHeader {
                public_key: inner.my_id.0,
                extensions: DocumentExtensions {
                    version: 1,
                    actor_id: inner.my_id,
                },
            },
            body: DocumentBody::Member { key_bundle },
            hash: Hash::from_bytes(inner.rng.random_array()?),
        })
    }

    pub fn process(&mut self, operation: &FakeOperation<C>) -> Result<(), UniverseError<C, GS>> {
        // ... observes messages on the network (scoped by topic id)

        // Orderer comes here!
        //
        // - It needs to be here, right at the beginning, it knows about multiple documents
        // - TODO: Can it even be _outside_ all of this? Shouldn't the orderer be part of
        // `p2panda-stream`?

        // Router comes here!
        //
        // "routing logic" draft:
        // Is group control message or document control message?
        //    If group control message: Is it related to any documents I'm part of?
        //       If yes, route it to the regarding document processors
        //       In any case, always route it to the regarding group processor
        //   If document control message: Are you part of the document?
        //       If not, keep it around and wait
        //       If yes, route it to the regarding document processor
        //
        // Directing every control message to the regarding document(s).
        // - this means that the router needs to understand which document relates to what groups ..
        // - If we are already inside the group (via CREATE or ADD), then the router forwards it
        // directly to the regarding documents. If NOT, then the router keeps them, and re-plays the
        // whole graph when we're welcomed
        // - In any case, if you're not inside any group, we're still processing them for establishing
        // group state (outside of documents / encryption).

        // Key Registry
        //
        // - We also observe published key bundle messages in the network. If they're not expired we
        // also store them in some sort of key registry.

        // Validation?
        //
        // - Extra rules around what to process first

        todo!()
    }

    async fn register_key_bundle(
        &mut self,
        id: ActorId,
        key_bundle: LongTermKeyBundle,
    ) -> Result<(), UniverseError<C, GS>> {
        // Reject expired and invalid key bundles.
        key_bundle.verify()?;

        let mut inner = self.inner.write().await;
        inner.pki = KeyRegistry::register_key_bundle(inner.pki.clone(), id, key_bundle);

        Ok(())
    }

    async fn identify_actor_types(
        &self,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<Vec<(GroupMember<ActorId>, Access<C>)>, UniverseError<C, GS>> {
        let inner = self.inner.read().await;
        let mut result = Vec::with_capacity(initial_members.len());

        for (id, access) in initial_members {
            if inner.groups.contains_key(id) {
                result.push((GroupMember::Group(*id), access.clone()));
            } else if inner.individuals.contains(id) {
                result.push((GroupMember::Individual(*id), access.clone()));
            } else if inner.documents.contains_key(id) {
                return Err(UniverseError::DocumentIsNotMember(*id));
            } else {
                return Err(UniverseError::UnknownActor(*id));
            }
        }

        Ok(result)
    }
}

impl<C, GS> Clone for Universe<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum UniverseError<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    #[error("documents like {0} can not be members of groups")]
    DocumentIsNotMember(ActorId),

    // This happens if our universe did not observe the key bundle for this member yet or a create
    // message for a group.
    //
    // Or the actor id is simply not existant.
    #[error(
        "actor {0} is unknown and can not be added. we might be missing key bundles or a group creation"
    )]
    UnknownActor(ActorId),

    #[error(transparent)]
    Group(#[from] GroupError),

    #[error(transparent)]
    Document(#[from] DocumentError<C, GS>),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error(transparent)]
    KeyBundle(#[from] KeyBundleError),

    #[error(transparent)]
    Rng(#[from] RngError),
}

#[derive(Debug, Error)]
pub enum DocumentError<C, GS>
where
    C: fmt::Debug + Clone + PartialOrd + 'static,
    GS: GroupStore<ActorId, Group = AuthGroupState<C, GS>> + Clone + fmt::Debug + 'static,
{
    #[error("tried to access a document {0} which is not known to us")]
    UnknownDocument(ActorId),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error(transparent)]
    EncryptionGroup(#[from] EncryptionGroupError<C, GS>),

    #[error(transparent)]
    Rng(#[from] RngError),
    //
    // TODO: Requires C to implement Display?
    // TODO: Causes infinite cycle ..?
    // #[error(transparent)]
    // AuthGroup(#[from] AuthGroupError<C, GS>),
}

#[derive(Debug, Error)]
pub enum GroupError {
    #[error(transparent)]
    Rng(#[from] RngError),
}

fn secret_members<C>(members: Vec<(ActorId, Access<C>)>) -> Vec<ActorId> {
    members
        .into_iter()
        .filter_map(|(id, access)| match access {
            Access::Pull => None,
            Access::Read | Access::Write { .. } | Access::Manage => Some(id),
        })
        .collect()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs()
}

#[cfg(test)]
mod tests {
    // ~~~~~~~~~~~
    // Group Store
    // ~~~~~~~~~~~

    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::convert::Infallible;
    use std::rc::Rc;

    use p2panda_auth::group::{Access, GroupMember};
    use p2panda_auth::traits::GroupStore;
    use p2panda_core::PrivateKey;
    use p2panda_encryption::Rng;

    use super::{ActorId, AuthGroupState, Universe, UniverseConfig};

    #[derive(Debug, Clone)]
    pub struct SqliteStore(Rc<RefCell<HashMap<ActorId, AuthGroupState<Conditions, Self>>>>);

    impl SqliteStore {
        pub fn new() -> Self {
            Self(Rc::new(RefCell::new(HashMap::new())))
        }
    }

    impl GroupStore<ActorId> for SqliteStore {
        type Group = AuthGroupState<Conditions, Self>;

        type Error = Infallible;

        // TODO: Should be an atomic write transaction instead.
        fn insert(&self, id: &ActorId, group: &Self::Group) -> Result<(), Self::Error> {
            {
                let mut store = self.0.borrow_mut();
                // TODO: We've enabled the `test_utils` flag right now to make this cloning work.
                // That should not be required as soon as `insert` gets removed from the store and
                // we use atomic transactions instead.
                store.insert(*id, group.clone());
            }
            Ok(())
        }

        fn get(&self, id: &ActorId) -> Result<Option<Self::Group>, Self::Error> {
            let store = self.0.borrow();
            let group_y = store.get(id);
            Ok(group_y.cloned())
        }
    }

    type Conditions = ();

    #[tokio::test]
    async fn it_works() {
        // TODO: Make resolver generic again.
        // let resolver = GroupResolver::default();

        // ----------------

        // A "universe" holding all state for alice's laptop!
        let mut alice_universe = {
            let rng = Rng::default();
            let store = SqliteStore::new();
            let config = UniverseConfig::default();

            let private_key = PrivateKey::new();
            let my_id = ActorId(private_key.public_key());

            Universe::<Conditions, SqliteStore>::new(my_id, config, store, rng).unwrap()
        };
        let alice_laptop_id = alice_universe.id().await;

        // Another one!
        let mut bob_universe = {
            let rng = Rng::default();
            let store = SqliteStore::new();
            let config = UniverseConfig::default();

            let private_key = PrivateKey::new();
            let my_id = ActorId(private_key.public_key());

            Universe::<Conditions, SqliteStore>::new(my_id, config, store, rng).unwrap()
        };
        let bob_smartphone_id = bob_universe.id().await;

        // ----------------

        let alice_operation_0 = alice_universe.key_bundle().await.unwrap();

        let (alice, alice_operation_1) = alice_universe
            .create_group(&[(alice_laptop_id, Access::Manage)])
            .await
            .unwrap();

        let (document, alice_operation_2) = alice_universe
            .create_document(&[(alice.id(), Access::Write { conditions: None })])
            .await
            .unwrap();

        // ----------------

        bob_universe.process(&alice_operation_0);

        // ----------------

        // TODO: Later we want to do this (after a user action or processing).
        // operation_1.write(&mut tx).await.unwrap();
        // universe.write(&mut tx).await.unwrap();
        // etc.
        // tx.commit().await.unwrap();
    }
}
