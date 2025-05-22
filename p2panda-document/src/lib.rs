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

use p2panda_auth::group::resolver::GroupResolver;
use p2panda_auth::group::{
    Group as AuthGroupGeneric, GroupAction, GroupControlMessage, GroupMember,
    GroupState as AuthGroupStateGeneric, GroupStateInner,
};
use p2panda_auth::group_crdt::Access;
use p2panda_auth::traits::{
    AuthGraph, GroupStore, IdentityHandle as AuthIdentityHandle, Operation as AuthMessage,
    OperationId as AuthOperationId, Ordering as AuthOrdering,
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
    IdentityHandle as EncryptionIdentityHandle, IdentityManager, IdentityRegistry,
    OperationId as EncryptionOperationId, Ordering as EncryptionOrdering, PreKeyManager,
    PreKeyRegistry,
};
use p2panda_encryption::{
    KeyManager as KeyManagerInner, KeyManagerError, KeyManagerState as KeyManagerStateInner,
    KeyRegistry as KeyRegistryInner, KeyRegistryState as KeyRegistryStateInner, Lifetime,
    LongTermKeyBundle, Rng, RngError,
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

// TODO: Making the resolver generic causes an type cycle overflow, so we "hardcode" it here for now.
pub type AuthResolver<C> = GroupResolver<ActorId, OperationId, Message<C>>;

pub type AuthGroup<C, GS> =
    AuthGroupGeneric<ActorId, OperationId, AuthResolver<C>, Orderer<C, GS>, GS>;

pub type AuthGroupState<C, GS> =
    AuthGroupStateGeneric<ActorId, OperationId, AuthResolver<C>, Orderer<C, GS>, GS>;

// TODO: This will probably be removed soon?
pub type AuthGroupStateInner<C> = GroupStateInner<ActorId, OperationId, Message<C>>;

pub type AuthControlMessage = GroupControlMessage<ActorId, OperationId>;

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

impl<C, GS> AuthOrdering<ActorId, OperationId, AuthControlMessage> for Orderer<C, GS>
where
    C: fmt::Debug + Clone + 'static,
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + fmt::Debug + Clone + 'static,
{
    type State = OrdererState;

    type Message = Message<C>;

    type Error = DocumentError<C, GS>;

    fn next_message(
        y: Self::State,
        control_message: &AuthControlMessage,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let sender = y.my_id;
        Ok((
            y,
            Message::PreAuth {
                sender,
                document_id: control_message.group_id(),
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

#[derive(Clone, Debug)]
pub struct FakeOperation<C> {
    header: FakeHeader,
    body: DocumentBody<C>,
    hash: Hash,
}

#[derive(Clone, Debug)]
pub enum DocumentControlMessage<C> {
    Create {
        initial_members: Vec<(ActorId, Access<C>)>,
    },
}

#[derive(Clone, Debug)]
pub struct DocumentBody<C> {
    control_message: DocumentControlMessage<C>,
    direct_messages: Vec<EncryptionDirectMessage>,
}

#[derive(Clone, Debug)]
pub struct DocumentExtensions {
    version: u64,
    document_id: ActorId,
}

// ~~~~~~~
// Message
// ~~~~~~~

#[derive(Clone, Debug)]
pub enum Message<C> {
    PreAuth {
        sender: ActorId,
        document_id: ActorId,
        control_message: AuthControlMessage,
    },
    PreEncryption {
        sender: ActorId,
        control_message: EncryptionControlMessage<ActorId>,
        direct_messages: Vec<EncryptionDirectMessage>,
    },
    Signed(FakeOperation<C>),
}

impl<C> AuthMessage<ActorId, OperationId, AuthControlMessage> for Message<C>
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

    fn payload(&self) -> AuthControlMessage {
        let message = match self {
            Message::Signed(operation) => match operation.body.control_message {
                DocumentControlMessage::Create {
                    ref initial_members,
                } => AuthControlMessage::GroupAction {
                    group_id: operation.header.extensions.document_id,
                    action: GroupAction::Create {
                        // TODO: Question how to bring back the group member type (individual or
                        // sub group) back here from the message.
                        //
                        // We could encode it in the message, but it would still need to be checked
                        // anyhow an receiving.
                        //
                        // Probably we want to ask the universe state here and resolve the types.
                        initial_members: erase_generic_hack(&define_group_type_hack(
                            initial_members,
                        )),
                    },
                },
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
    C: fmt::Debug + Clone + 'static,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + fmt::Debug + Clone + 'static,
{
    id: ActorId,
    universe: Universe<C, GS>,
    _marker: PhantomData<C>,
}

impl<C, GS> Group<C, GS>
where
    C: fmt::Debug + Clone + 'static,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + fmt::Debug + Clone + 'static,
{
    pub fn id(&self) -> ActorId {
        self.id
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
    C: fmt::Debug + Clone + 'static,
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + fmt::Debug + Clone + 'static,
{
    id: ActorId,
    universe: Universe<C, GS>,
    _marker: PhantomData<C>,
}

pub struct DocumentState<C, GS>
where
    C: fmt::Debug + Clone + 'static,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + fmt::Debug + Clone + 'static,
{
    auth_state: AuthGroupState<C, GS>,
    encryption_state: EncryptionGroupState<C, GS>,
}

impl<C, GS> Document<C, GS>
where
    // TODO: Clone and Debug bound for both RS and GS is maybe not necessary?
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + Clone + fmt::Debug,
    C: fmt::Debug + Clone + 'static,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + Clone + fmt::Debug + 'static,
{
    pub(crate) async fn create(
        universe_owned: Universe<C, GS>,
        initial_members: &[(GroupMember<ActorId>, Access<C>)],
    ) -> Result<(Document<C, GS>, DocumentState<C, GS>), DocumentError<C, GS>> {
        let universe = universe_owned.inner.read().await;

        // TODO: Here something happens with deriving a group id.
        let document_id = ActorId(PrivateKey::new().public_key());

        let (auth_state, auth_pre_message) = {
            let y = AuthGroupState::new(
                universe.my_id,
                document_id,
                universe.group_store_state.clone(), // TODO: This will probably change
                universe.orderer.clone(),
            );

            let control_message = AuthControlMessage::GroupAction {
                group_id: document_id,
                action: GroupAction::Create {
                    initial_members: erase_generic_hack(initial_members),
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
            let initial_members = secret_members(initial_members);

            let (y_ii, pre) = EncryptionGroup::create(y, initial_members, &universe.rng)?;

            (y_ii, pre)
        };

        let Message::PreAuth { document_id, .. } = auth_pre_message else {
            unreachable!("method will always return a pre-auth message");
        };

        let Message::PreEncryption {
            direct_messages, ..
        } = encryption_pre_message
        else {
            unreachable!("method will always return a pre-encryption message");
        };

        // TODO: Use real p2panda operations with extensions here & sign them.
        let operation = {
            let initial_members = erase_group_type(initial_members);

            Message::Signed(FakeOperation {
                header: FakeHeader {
                    public_key: universe.my_id.0,
                    extensions: DocumentExtensions {
                        version: 1,
                        document_id,
                    },
                },
                body: DocumentBody {
                    control_message: DocumentControlMessage::Create { initial_members },
                    direct_messages,
                },
                hash: Hash::from_bytes(universe.rng.random_array()?),
            })
        };

        let auth_state = {
            let y_ii = AuthGroup::process(auth_state, &operation).unwrap();
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
        ))
    }

    // We call this after receiving a CREATE or ADD which brings us into a document.
    pub(crate) fn from_welcome(
        &self,
        _group_id: ActorId,
        _group_store_state: GS::State,
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
        access: Access<()>,
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
                    // TODO: Access should use our C generic here.
                    access,
                },
            };

            // TODO: Clone bound on RS and ORD in `prepare` is confusing.
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

// Messages we can see on the network inside a "universe" (app scope usually).
pub enum UniverseMessage {
    KeyBundle,
    Group,
    Document,
}

pub struct InnerUniverse<C, GS>
where
    C: fmt::Debug + Clone + 'static,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + fmt::Debug + Clone + 'static,
{
    pub(crate) my_id: ActorId,

    pub(crate) private_key: PrivateKey,

    pub(crate) identity_secret: SecretKey,

    pub(crate) my_keys: KeyManagerState,

    // Here we have _all_ groups EXCEPT "root groups" / documents.
    pub(crate) groups: HashMap<ActorId, AuthGroupState<C, GS>>,

    // Here we have all "root groups" / documents, no "sub groups".
    pub(crate) documents: HashMap<ActorId, DocumentState<C, GS>>,

    // Key bundles.
    pub(crate) pki: KeyRegistryState,

    pub(crate) store: GS,

    pub(crate) group_store_state: GS::State,

    pub(crate) orderer: OrdererState,

    pub(crate) dgm: EncryptionGroupManagerState,

    pub(crate) rng: Rng,

    _marker: PhantomData<C>,
}

// "App Universe", that's the "orchestrator" managing multiple documents and groups.
pub struct Universe<C, GS>
where
    C: fmt::Debug + Clone + 'static,
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + fmt::Debug + Clone + 'static,
{
    pub(crate) inner: Arc<RwLock<InnerUniverse<C, GS>>>,
}

impl<C, GS> Universe<C, GS>
where
    C: fmt::Debug + Clone + 'static,
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + fmt::Debug + Clone,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + fmt::Debug + Clone + 'static,
{
    pub fn new(
        private_key: PrivateKey,
        store: GS,
        group_store_state: GS::State,
    ) -> Result<Self, UniverseError<C, GS>> {
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

        // TODO: Allow seedable rng for test environments.
        let rng = Rng::default();

        let my_id = ActorId(private_key.public_key());

        let identity_secret = SecretKey::from_rng(&rng)?;

        let dgm = EncryptionGroupManagerState::default();

        let orderer = OrdererState { my_id };

        let my_keys = KeyManager::init(
            &identity_secret,
            // TODO: Make lifetime configurable.
            Lifetime::default(),
            &rng,
        )?;

        Ok(Self {
            inner: Arc::new(RwLock::new(InnerUniverse {
                my_id,
                private_key,
                identity_secret,
                my_keys,
                groups: HashMap::new(),
                documents: HashMap::new(),
                pki: KeyRegistry::init(),
                store,
                group_store_state,
                orderer,
                dgm,
                rng,
                _marker: PhantomData,
            })),
        })
    }

    pub async fn id(&self) -> ActorId {
        let inner = self.inner.read().await;
        inner.my_id
    }

    pub async fn create_document(
        &mut self,
        initial_members: &[(GroupMember<ActorId>, Access<C>)],
    ) -> Result<Document<C, GS>, UniverseError<C, GS>> {
        let (document, y_doc) = Document::create(self.clone(), initial_members).await?;

        let mut inner = self.inner.write().await;
        inner.documents.insert(document.id(), y_doc);

        Ok(document)
    }

    pub async fn create_group(
        &mut self,
        initial_members: &[(GroupMember<ActorId>, Access<C>)],
    ) -> Result<Group<C, GS>, UniverseError<C, GS>> {
        todo!();

        // let (group, y_group) = Group::create(self.clone(), initial_members).await?;
        //
        // let mut inner = self.inner.write().await;
        // inner.groups.insert(group.id(), y_group);
        //
        // Ok(group)
    }

    pub fn process(&self) {
        // TODO
    }
}

impl<C, GS> Clone for Universe<C, GS>
where
    C: fmt::Debug + Clone,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + fmt::Debug + Clone + 'static,
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
    C: fmt::Debug + Clone + 'static,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + fmt::Debug + Clone + 'static,
{
    #[error(transparent)]
    Document(#[from] DocumentError<C, GS>),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error(transparent)]
    Rng(#[from] RngError),
}

#[derive(Debug, Error)]
pub enum DocumentError<C, GS>
where
    C: fmt::Debug + Clone + 'static,
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner<C>> + fmt::Debug + Clone + 'static,
{
    #[error("tried to access a document {0} which is not known to us")]
    UnknownDocument(ActorId),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error(transparent)]
    EncryptionGroup(#[from] EncryptionGroupError<C, GS>),

    #[error(transparent)]
    Rng(#[from] RngError),
    // TODO: We're hiding the error message here.
    // TODO: Having the resolver type mentioned in this error causes an infinite cycle which
    // overflows Rust.
    // #[error("group error occurred")]
    // Group(GroupError<ActorId, OperationId, AuthResolver, Orderer<GS>, GS>),
}

fn secret_members<C>(members: &[(GroupMember<ActorId>, Access<C>)]) -> Vec<ActorId> {
    members
        .iter()
        .filter_map(|(member, access)| match access {
            Access::Pull => None,
            Access::Read | Access::Write { .. } | Access::Manage => match member {
                GroupMember::Individual(id) => Some(id),
                GroupMember::Group { id } => Some(id),
            },
        })
        .cloned()
        .collect()
}

fn erase_group_type<C>(members: &[(GroupMember<ActorId>, Access<C>)]) -> Vec<(ActorId, Access<C>)>
where
    C: Clone,
{
    members
        .iter()
        .map(|(member, access)| {
            let member = match member {
                GroupMember::Individual(id) => id,
                GroupMember::Group { id } => id,
            };
            (member.to_owned(), access.to_owned())
        })
        .collect()
}

fn define_group_type_hack<C>(
    members: &[(ActorId, Access<C>)],
) -> Vec<(GroupMember<ActorId>, Access<C>)>
where
    C: Clone,
{
    members
        .iter()
        .map(|(member, access)| {
            (
                GroupMember::Individual(member.to_owned()),
                access.to_owned(),
            )
        })
        .collect()
}

// TODO: Manually erasing C generic here ..
fn erase_generic_hack<C>(
    members: &[(GroupMember<ActorId>, Access<C>)],
) -> Vec<(GroupMember<ActorId>, Access<()>)> {
    members
        .iter()
        .map(|(member, access)| {
            (
                member.to_owned(),
                match access {
                    Access::Pull => Access::Pull,
                    Access::Read => Access::Read,
                    Access::Write { .. } => Access::Write {
                        conditions: Some(()),
                    },
                    Access::Manage => Access::Manage,
                },
            )
        })
        .collect::<Vec<(GroupMember<ActorId>, Access<()>)>>()
        .to_vec()
}

#[cfg(test)]
mod tests {
    // ~~~~~~~~~~~
    // Group Store
    // ~~~~~~~~~~~

    use std::convert::Infallible;

    // use p2panda_auth::group::resolver::GroupResolver;
    use p2panda_auth::group::GroupMember;
    use p2panda_auth::group_crdt::Access;
    use p2panda_auth::traits::GroupStore;
    use p2panda_core::PrivateKey;

    use super::{ActorId, AuthGroupStateInner, Universe};

    #[derive(Debug, Clone)]
    pub struct SqliteStore;

    impl SqliteStore {
        pub fn new() -> Self {
            Self {}
        }
    }

    impl GroupStore<ActorId, AuthGroupStateInner<Conditions>> for SqliteStore {
        type State = ();

        type Error = Infallible;

        // TODO: No writes here.
        fn insert(
            y: Self::State,
            _id: &ActorId,
            _group: &AuthGroupStateInner<Conditions>,
        ) -> Result<Self::State, Self::Error> {
            // TODO: Noop
            Ok(y)
        }

        fn get(
            _y: &Self::State,
            _id: &ActorId,
        ) -> Result<Option<AuthGroupStateInner<Conditions>>, Self::Error> {
            todo!()
        }
    }

    type Conditions = ();

    #[tokio::test]
    async fn it_works() {
        let store = SqliteStore::new();

        let private_key = PrivateKey::new();

        // TODO: Make resolver generic again.
        // let resolver = GroupResolver::default();

        // A "universe" holding all state for alice's laptop!
        let mut universe =
            Universe::<Conditions, SqliteStore>::new(private_key, store, ()).unwrap();

        let alice = universe
            .create_group(&[(GroupMember::Individual(universe.id().await), Access::Manage)])
            .await
            .unwrap();

        let document = universe
            .create_document(&[(
                GroupMember::Group { id: alice.id() },
                Access::Write { conditions: None },
            )])
            .await
            .unwrap();

        // TODO: Later we want to do this (after a user action or processing).
        // universe.write(&mut tx).await.unwrap();
        // tx.commit().await.unwrap();
    }
}
