use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::fmt::{self, Display};
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_auth::group::resolver::GroupResolver;
use p2panda_auth::group::{
    Group, GroupAction, GroupControlMessage, GroupMember, GroupState as AuthGroupStateGeneric,
    GroupStateInner,
};
use p2panda_auth::group_crdt::Access;
use p2panda_auth::traits::{
    AuthGraph, GroupStore, IdentityHandle as AuthIdentityHandle, Operation as AuthMessage,
    OperationId as AuthOperationId, Ordering as AuthOrdering,
};
use p2panda_core::{Hash, Operation, PrivateKey, PublicKey};
use p2panda_encryption::crypto::{SecretKey, XAeadNonce};
use p2panda_encryption::data_scheme::{
    ControlMessage as EncryptionControlMessage, DirectMessage as EncryptionDirectMessage,
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
    KeyManager, KeyManagerError, KeyManagerState, KeyRegistry as KeyRegistryInner,
    KeyRegistryState as KeyRegistryStateInner, Lifetime, LongTermKeyBundle, Rng, RngError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

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

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// Key manager & registry wrappers
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

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
        let mut inner = y.inner.blocking_lock();
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
        let inner = y.inner.blocking_lock();
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
        let inner = self.inner.blocking_lock();
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
pub type AuthResolver = GroupResolver<ActorId, OperationId, Message>;

pub type AuthGroup<GS> = Group<ActorId, OperationId, AuthResolver, Orderer<GS>, GS>;

pub type AuthGroupState<GS> =
    AuthGroupStateGeneric<ActorId, OperationId, AuthResolver, Orderer<GS>, GS>;

// TODO: This will probably be removed soon?
pub type AuthGroupStateInner = GroupStateInner<ActorId, OperationId, Message>;

pub type AuthControlMessage = GroupControlMessage<ActorId, OperationId>;

pub type EncryptionGroupState<GS> = EncryptionGroupStateGeneric<
    ActorId,
    OperationId,
    KeyRegistry,
    EncryptionGroupManager,
    KeyManager,
    Orderer<GS>,
>;

pub type EncryptionGroupError<GS> = EncryptionGroupErrorGeneric<
    ActorId,
    OperationId,
    KeyRegistry,
    EncryptionGroupManager,
    KeyManager,
    Orderer<GS>,
>;

// ~~~~~~~~~~~~~~
// Encryption DGM
// ~~~~~~~~~~~~~~

#[derive(Debug)]
pub struct EncryptionGroupManager {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptionGroupManagerState {}

impl EncryptionGroupManagerState {
    pub fn init() -> Self {
        Self {}
    }
}

impl GroupMembership<ActorId, OperationId> for EncryptionGroupManager {
    type State = EncryptionGroupManagerState;

    type Error = Infallible;

    fn create(_my_id: ActorId, _initial_members: &[ActorId]) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn from_welcome(_my_id: ActorId, _y: Self::State) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn add(
        _y: Self::State,
        _adder: ActorId,
        _added: ActorId,
        _operation_id: OperationId,
    ) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn remove(
        _y: Self::State,
        _remover: ActorId,
        _removed: &ActorId,
        _operation_id: OperationId,
    ) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn members(_y: &Self::State) -> Result<HashSet<ActorId>, Self::Error> {
        todo!()
    }
}

// ~~~~~~~
// Orderer
// ~~~~~~~

#[derive(Clone, Debug)]
pub struct Orderer<GS> {
    _marker: PhantomData<GS>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrdererState {}

impl<GS> AuthOrdering<ActorId, OperationId, AuthControlMessage> for Orderer<GS>
where
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone + 'static,
{
    type State = OrdererState;

    type Message = Message;

    type Error = DocumentError<GS>;

    fn next_message(
        y: Self::State,
        control_message: &AuthControlMessage,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        Ok((
            y,
            Message::Pre {
                document_id: control_message.group_id(),
                auth_control_message: Some(control_message.to_owned()),
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

impl<GS> EncryptionOrdering<ActorId, OperationId, EncryptionGroupManager> for Orderer<GS> {
    type State = OrdererState;

    type Error = Infallible;

    type Message = Message;

    fn next_control_message(
        _y: Self::State,
        _control_message: &EncryptionControlMessage<ActorId>,
        _direct_messages: &[EncryptionDirectMessage<
            ActorId,
            OperationId,
            EncryptionGroupManager,
        >],
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        todo!()
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

    fn set_welcome(_y: Self::State, _message: &Self::Message) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn next_ready_message(
        _y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        todo!()
    }
}

// ~~~~~~~
// Message
// ~~~~~~~

#[derive(Clone, Debug)]
pub enum Message {
    Pre {
        document_id: ActorId,
        auth_control_message: Option<AuthControlMessage>,
    },
    Signed(Operation<()>),
}

impl AuthMessage<ActorId, OperationId, AuthControlMessage> for Message {
    fn id(&self) -> OperationId {
        match self {
            Message::Pre { .. } => {
                unreachable!("should never call id on message in unsigned pre-state")
            }
            Message::Signed(operation) => OperationId(operation.hash),
        }
    }

    fn sender(&self) -> ActorId {
        match self {
            Message::Pre { .. } => {
                unreachable!("should never call sender on message in unsigned pre-state")
            }
            Message::Signed(operation) => ActorId(operation.header.public_key),
        }
    }

    fn dependencies(&self) -> &Vec<OperationId> {
        todo!()
    }

    fn previous(&self) -> &Vec<OperationId> {
        todo!()
    }

    fn payload(&self) -> &AuthControlMessage {
        todo!()
    }
}

impl EncryptionMessage<ActorId, OperationId, EncryptionGroupManager> for Message {
    fn id(&self) -> OperationId {
        todo!()
    }

    fn sender(&self) -> ActorId {
        todo!()
    }

    fn message_type(&self) -> GroupMessageType<ActorId> {
        todo!()
    }

    fn direct_messages(
        &self,
    ) -> Vec<EncryptionDirectMessage<ActorId, OperationId, EncryptionGroupManager>> {
        todo!()
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
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone + 'static,
{
    id: ActorId,
    universe: Universe<C, GS>,
    _marker: PhantomData<C>,
}

pub struct DocumentState<GS>
where
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone + 'static,
{
    auth_state: AuthGroupState<GS>,
    encryption_state: EncryptionGroupState<GS>,
}

impl<C, GS> Document<C, GS>
where
    // TODO: Clone and Debug bound for both RS and GS is maybe not necessary?
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + Clone + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner> + Clone + fmt::Debug + 'static,
{
    pub(crate) async fn create(
        universe: Universe<C, GS>,
        initial_members: &[(GroupMember<ActorId>, Access<()>)],
    ) -> Result<(Document<C, GS>, DocumentState<GS>), DocumentError<GS>> {
        // TODO: Here something happens with deriving a group id.
        let document_id = ActorId(PrivateKey::new().public_key());

        let auth_state = {
            let universe = universe.inner.read().await;

            let y = AuthGroupState::new(
                universe.my_id,
                document_id,
                universe.group_store_state.clone(), // TODO: This will probably change
                universe.orderer.clone(),
            );

            let control_message = AuthControlMessage::GroupAction {
                group_id: document_id,
                action: GroupAction::Create {
                    initial_members: initial_members.to_vec(),
                },
            };

            // TODO: We can't handle the error yet (see `DocumentError`).
            let (y_i, pre) = AuthGroup::prepare(y, &control_message).unwrap(); //map_err(DocumentError::Group)?;

            // TODO: Set up encryption state.

            // TODO: Create & sign p2panda operation here.
            // let operation = ...

            let y_ii = AuthGroup::process(y_i, &pre).unwrap(); //.map_err(DocumentError::Group)?;

            // TODO: Process encryption group as well.

            y_ii
        };

        let encryption_state = {
            let universe = universe.inner.read().await;

            let my_keys = KeyManager::init(
                &universe.identity_secret,
                // TODO: Make lifetime configurable.
                Lifetime::default(),
                &universe.rng,
            )?;

            let y = EncryptionGroup::init(
                universe.my_id,
                my_keys,
                universe.pki.clone(),
                universe.dgm.clone(),
                universe.orderer.clone(),
            );

            let (y_ii, _) = EncryptionGroup::create(y, vec![], &universe.rng)?;

            y_ii
        };

        Ok((
            Document {
                id: document_id,
                universe,
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
    ) -> Result<AuthGroupState<GS>, DocumentError<GS>> {
        todo!()
    }

    pub fn id(&self) -> ActorId {
        self.id
    }

    pub async fn add(
        &self,
        added: GroupMember<ActorId>,
        access: Access<()>,
    ) -> Result<(), DocumentError<GS>> {
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
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone + 'static,
{
    pub(crate) my_id: ActorId,

    pub(crate) private_key: PrivateKey,

    pub(crate) identity_secret: SecretKey,

    // Here we have _all_ groups EXCEPT "root groups" / documents.
    pub(crate) groups: HashMap<ActorId, AuthGroupState<GS>>,

    // Here we have all "root groups" / documents, no "sub groups".
    pub(crate) documents: HashMap<ActorId, DocumentState<GS>>,

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
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone + 'static,
{
    pub(crate) inner: Arc<RwLock<InnerUniverse<C, GS>>>,
}

impl<C, GS> Universe<C, GS>
where
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + fmt::Debug + Clone,
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone + 'static,
{
    pub fn new(
        private_key: PrivateKey,
        store: GS,
        group_store_state: GS::State,
    ) -> Result<Self, UniverseError<GS>> {
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

        // TODO: Secret key should be persisted and not newly generated every time.
        let identity_secret = SecretKey::from_rng(&rng)?;

        let dgm = EncryptionGroupManagerState::init();

        let orderer = OrdererState {};

        Ok(Self {
            inner: Arc::new(RwLock::new(InnerUniverse {
                my_id,
                private_key,
                identity_secret,
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

    pub async fn create_document(
        &mut self,
        initial_members: &[(GroupMember<ActorId>, Access<()>)],
    ) -> Result<Document<C, GS>, UniverseError<GS>> {
        let (document, y_doc) = Document::create(self.clone(), initial_members).await?;

        let mut inner = self.inner.write().await;
        inner.documents.insert(document.id(), y_doc);

        Ok(document)
    }

    pub fn create_group(&self) {
        // TODO
    }

    pub fn process(&self) {
        // TODO

        // TODO
        // Yields events:
        // - Has a group been created / updated
        // - Has a document been created / updated
        // - Have we been invited somewhere
        // - Have we been removed somewhere
        // - Did we receive some decrypted application data
    }
}

impl<C, GS> Clone for Universe<C, GS>
where
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum UniverseError<GS>
where
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone + 'static,
{
    #[error("")]
    Document(#[from] DocumentError<GS>),

    #[error(transparent)]
    Rng(#[from] RngError),
}

#[derive(Debug, Error)]
pub enum DocumentError<GS>
where
    // RS: Resolver<AuthGroupState<RS, GS>, Message> + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone + 'static,
{
    #[error("tried to access a document {0} which is not known to us")]
    UnknownDocument(ActorId),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error("encryption group failed: {0}")]
    EncryptionGroup(#[from] EncryptionGroupError<GS>),
    // TODO: We're hiding the error message here.
    // TODO: Having the resolver type mentioned in this error causes an infinite cycle which
    // overflows Rust.
    // #[error("group error occurred")]
    // Group(GroupError<ActorId, OperationId, AuthResolver, Orderer<GS>, GS>),
}

#[cfg(test)]
mod tests {
    // ~~~~~~~~~~~
    // Group Store
    // ~~~~~~~~~~~

    use std::convert::Infallible;

    // use p2panda_auth::group::resolver::GroupResolver;
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

    impl GroupStore<ActorId, AuthGroupStateInner> for SqliteStore {
        type State = ();

        type Error = Infallible;

        // TODO: No writes here.
        fn insert(
            _y: Self::State,
            _id: &ActorId,
            _group: &AuthGroupStateInner,
        ) -> Result<Self::State, Self::Error> {
            todo!()
        }

        fn get(
            _y: &Self::State,
            _id: &ActorId,
        ) -> Result<Option<AuthGroupStateInner>, Self::Error> {
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

        let mut universe =
            Universe::<Conditions, SqliteStore>::new(private_key, store, ()).unwrap();

        let _document = universe.create_document(&[]).await.unwrap();

        // TODO: Later we want to do this (after a user action or processing).
        // universe.write(&mut tx).await.unwrap();
        // tx.commit().await.unwrap();
    }
}
