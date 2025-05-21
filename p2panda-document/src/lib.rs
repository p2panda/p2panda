use std::collections::HashMap;
use std::fmt::{self, Display};
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_auth::group::resolver::GroupResolver;
use p2panda_auth::group::{
    Group, GroupAction, GroupControlMessage, GroupMember, GroupState, GroupStateInner,
};
use p2panda_auth::group_crdt::Access;
use p2panda_auth::traits::Ordering as AuthOrdering;
use p2panda_auth::traits::{
    AuthGraph, GroupStore, IdentityHandle as AuthIdentityHandle, Operation as AuthOperation,
    OperationId,
};
use p2panda_core::{Hash, PrivateKey, PublicKey};
use p2panda_encryption::traits::IdentityHandle as EncryptionIdentityHandle;
use p2panda_encryption::{KeyRegistry, KeyRegistryState};
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
pub struct MessageId(pub Hash);

impl Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl OperationId for MessageId {}

// ~~~~~~~~~~~~~~~~
// Auth group types
// ~~~~~~~~~~~~~~~~

// TODO: Making the resolver generic causes an type cycle overflow, so we "hardcode" it here for now.
pub type AuthResolver = GroupResolver<ActorId, MessageId, DocumentMessage>;

pub type AuthGroup<GS> = Group<ActorId, MessageId, AuthResolver, Orderer<GS>, GS>;

pub type AuthGroupState<GS> = GroupState<ActorId, MessageId, AuthResolver, Orderer<GS>, GS>;

// TODO: This will probably be removed soon?
pub type AuthGroupStateInner = GroupStateInner<ActorId, MessageId, DocumentMessage>;

pub type AuthControlMessage = GroupControlMessage<ActorId, MessageId>;

// ~~~~~~~
// Orderer
// ~~~~~~~

#[derive(Clone, Debug)]
pub struct Orderer<GS> {
    _marker: PhantomData<GS>,
}

#[derive(Clone, Debug)]
pub struct OrdererState {}

impl<GS> AuthOrdering<ActorId, MessageId, AuthControlMessage> for Orderer<GS>
where
    // RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    type State = OrdererState;

    type Message = DocumentMessage;

    type Error = DocumentError;

    fn next_message(
        _y: Self::State,
        _control_message: &AuthControlMessage,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        todo!()
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

// ~~~~~~~
// Message
// ~~~~~~~

#[derive(Clone, Debug)]
pub struct DocumentMessage {}

impl AuthOperation<ActorId, MessageId, AuthControlMessage> for DocumentMessage {
    fn id(&self) -> MessageId {
        todo!()
    }

    fn sender(&self) -> ActorId {
        todo!()
    }

    fn dependencies(&self) -> &Vec<MessageId> {
        todo!()
    }

    fn previous(&self) -> &Vec<MessageId> {
        todo!()
    }

    fn payload(&self) -> &AuthControlMessage {
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
    // RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    id: ActorId,
    universe: Universe<C, GS>,
    _marker: PhantomData<C>,
}

pub struct DocumentState<GS>
where
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    auth_state: AuthGroupState<GS>,
}

impl<C, GS> Document<C, GS>
where
    // TODO: Clone and Debug bound for both RS and GS is maybe not necessary?
    // RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + Clone + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner> + Clone + fmt::Debug,
{
    pub(crate) async fn create(
        universe: Universe<C, GS>,
        initial_members: &[(GroupMember<ActorId>, Access<()>)],
    ) -> Result<(Document<C, GS>, DocumentState<GS>), DocumentError> {
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
            let (y_i, operation) = AuthGroup::prepare(y, &control_message).unwrap(); //map_err(DocumentError::Group)?;
            let y_ii = AuthGroup::process(y_i, &operation).unwrap(); //.map_err(DocumentError::Group)?;

            y_ii
        };

        Ok((
            Document {
                id: document_id,
                universe,
                _marker: PhantomData,
            },
            DocumentState { auth_state },
        ))
    }

    // We call this after receiving a CREATE or ADD which brings us into a document.
    pub(crate) fn from_welcome(
        &self,
        _group_id: ActorId,
        _group_store_state: GS::State,
        _orderer: OrdererState,
    ) -> Result<AuthGroupState<GS>, DocumentError> {
        todo!()
    }

    pub fn id(&self) -> ActorId {
        self.id
    }

    pub async fn add(
        &self,
        added: GroupMember<ActorId>,
        access: Access<()>,
    ) -> Result<(), DocumentError> {
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
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    pub(crate) my_id: ActorId,

    // Here we have _all_ groups EXCEPT "root groups" / documents.
    pub(crate) groups: HashMap<ActorId, AuthGroupState<GS>>,

    // Here we have all "root groups" / documents, no "sub groups".
    pub(crate) documents: HashMap<ActorId, DocumentState<GS>>,

    // Key bundles.
    pub(crate) key_registry: KeyRegistryState<ActorId>,

    pub(crate) store: GS,

    pub(crate) group_store_state: GS::State,

    pub(crate) orderer: OrdererState,

    _marker: PhantomData<C>,
}

// "App Universe", that's the "orchestrator" managing multiple documents and groups.
pub struct Universe<C, GS>
where
    // RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + fmt::Debug,
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    pub(crate) inner: Arc<RwLock<InnerUniverse<C, GS>>>,
}

impl<C, GS> Universe<C, GS>
where
    // RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + fmt::Debug + Clone,
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    pub fn new(store: GS, group_store_state: GS::State) -> Self {
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

        // TODO: Get private key from the outside.
        let my_id = ActorId(PrivateKey::new().public_key());

        let orderer = OrdererState {};

        Self {
            inner: Arc::new(RwLock::new(InnerUniverse {
                my_id,
                groups: HashMap::new(),
                documents: HashMap::new(),
                key_registry: KeyRegistry::init(),
                store,
                group_store_state,
                orderer,
                _marker: PhantomData,
            })),
        }
    }

    pub async fn create_document(
        &mut self,
        initial_members: &[(GroupMember<ActorId>, Access<()>)],
    ) -> Result<Document<C, GS>, UniverseError> {
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
    GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum UniverseError {
    #[error(transparent)]
    Document(#[from] DocumentError),
}

#[derive(Debug, Error)]
pub enum DocumentError
// where
// RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + fmt::Debug,
// GS: GroupStore<ActorId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    #[error("tried to access a document {0} which is not known to us")]
    UnknownDocument(ActorId),
    // TODO: We're hiding the error message here.
    // TODO: Having the resolver type mentioned in this error causes an infinite cycle which
    // overflows Rust.
    // #[error("group error occurred")]
    // Group(GroupError<ActorId, MessageId, AuthResolver, Orderer<GS>, GS>),
}

#[cfg(test)]
mod tests {
    // ~~~~~~~~~~~
    // Group Store
    // ~~~~~~~~~~~

    use std::convert::Infallible;

    // use p2panda_auth::group::resolver::GroupResolver;
    use p2panda_auth::traits::GroupStore;

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

        // TODO: Make resolver generic again.
        // let resolver = GroupResolver::default();

        let mut universe = Universe::<Conditions, SqliteStore>::new(store, ());

        let _document = universe.create_document(&[]).await.unwrap();

        // TODO: Later we want to do this (after a user action or processing).
        // universe.write(&mut tx).await.unwrap();
        // tx.commit().await.unwrap();
    }
}
