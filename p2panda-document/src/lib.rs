use std::collections::HashMap;
use std::fmt::{self, Display};
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

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

// ~~~~~~~~~~
// Core types
// ~~~~~~~~~~

// Can be both a group id or individual id.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, StdHash, Serialize, Deserialize)]
pub struct MemberId(pub PublicKey);

impl Display for MemberId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AuthIdentityHandle for MemberId {}

impl EncryptionIdentityHandle for MemberId {}

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
pub type AuthResolver = GroupResolver<MemberId, MessageId, DocumentMessage>;

pub type AuthGroup<GS> = Group<MemberId, MessageId, AuthResolver, DocumentOrderer<GS>, GS>;

pub type AuthGroupState<GS> =
    GroupState<MemberId, MessageId, AuthResolver, DocumentOrderer<GS>, GS>;

// TODO: This will probably be removed soon?
pub type AuthGroupStateInner = GroupStateInner<MemberId, MessageId, DocumentMessage>;

pub type AuthControlMessage = GroupControlMessage<MemberId, MessageId>;

// ~~~~~~~
// Orderer
// ~~~~~~~

#[derive(Clone, Debug)]
pub struct DocumentOrderer<GS> {
    _marker: PhantomData<GS>,
}

#[derive(Clone, Debug)]
pub struct DocumentOrdererState {}

impl<GS> AuthOrdering<MemberId, MessageId, AuthControlMessage> for DocumentOrderer<GS>
where
    // RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + fmt::Debug,
    GS: GroupStore<MemberId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    type State = DocumentOrdererState;

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

impl AuthOperation<MemberId, MessageId, AuthControlMessage> for DocumentMessage {
    fn id(&self) -> MessageId {
        todo!()
    }

    fn sender(&self) -> MemberId {
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

pub struct Document<C, GS>
where
    // RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + fmt::Debug,
    GS: GroupStore<MemberId, AuthGroupStateInner> + fmt::Debug,
{
    my_id: MemberId,
    group_store: GS,
    _marker: PhantomData<C>,
}

impl<C, GS> Document<C, GS>
where
    // TODO: Clone and Debug bound for both RS and GS is maybe not necessary?
    // RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + Clone + fmt::Debug,
    GS: GroupStore<MemberId, AuthGroupStateInner> + Clone + fmt::Debug,
{
    pub fn new(my_id: MemberId, group_store: GS) -> Self {
        Self {
            my_id,
            group_store,
            _marker: PhantomData,
        }
    }

    pub fn create(
        &self,
        initial_members: &[(GroupMember<MemberId>, Access<()>)],
        group_store_state: GS::State,
        orderer: DocumentOrdererState,
    ) -> Result<AuthGroupState<GS>, DocumentError> {
        // TODO: Here something happens with deriving a group id.
        let group_id = MemberId(PrivateKey::new().public_key());

        let y = AuthGroupState::new(self.my_id, group_id, group_store_state, orderer);

        let control_message = AuthControlMessage::GroupAction {
            group_id,
            action: GroupAction::Create {
                initial_members: initial_members.to_vec(),
            },
        };

        // TODO: We can't handle the error yet (see `DocumentError`).
        let (y_i, operation) = AuthGroup::prepare(y, &control_message).unwrap(); //map_err(DocumentError::Group)?;
        let y_ii = AuthGroup::process(y_i, &operation).unwrap(); //.map_err(DocumentError::Group)?;

        Ok(y_ii)
    }

    // We call this after receiving a CREATE or ADD which brings us into a document.
    pub fn from_welcome(
        &self,
        _group_id: MemberId,
        _group_store_state: GS::State,
        _orderer: DocumentOrdererState,
    ) -> Result<AuthGroupState<GS>, DocumentError> {
        todo!()
    }

    pub fn add(
        &self,
        y: AuthGroupState<GS>,
        added: GroupMember<MemberId>,
        access: Access<()>,
    ) -> Result<AuthGroupState<GS>, DocumentError> {
        // TODO: Basic checks here? Is this member already part of the group, do we try to add
        // ourselves, etc.?

        let control_message = AuthControlMessage::GroupAction {
            group_id: y.inner.group_id,
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
        // map_err(DocumentError::Group)?;

        Ok(y_ii)
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

// "App Universe", that's the "orchestrator" managing multiple documents and groups.
pub struct Universe<C, GS>
where
    // RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + fmt::Debug,
    GS: GroupStore<MemberId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    // Here we have _all_ groups EXCEPT "root groups" / documents.
    groups: HashMap<PublicKey, AuthGroupState<GS>>,

    // Here we have all "root groups" / documents, no "sub groups".
    documents: HashMap<PublicKey, Document<C, GS>>,

    // Key bundles.
    key_registry: KeyRegistryState<MemberId>,
}

impl<C, GS> Universe<C, GS>
where
    // RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + fmt::Debug + Clone,
    GS: GroupStore<MemberId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    pub fn new(conditions: C, store: GS) -> Self {
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
        // - TODO: Shouldn't this be handled outside as well? Across all "universes"?

        // Validation?
        //
        // - Extra rules around what to process first

        // TODO: Get private key from the outside.
        let my_id = MemberId(PrivateKey::new().public_key());

        let document: Document<C, GS> = Document::new(my_id, store);

        Self {
            groups: HashMap::new(),
            documents: HashMap::new(),
            key_registry: KeyRegistry::init(),
        }
    }
}

#[derive(Debug, Error)]
pub enum DocumentError
// where
// RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage> + fmt::Debug,
// GS: GroupStore<MemberId, AuthGroupStateInner> + fmt::Debug + Clone,
{
    // TODO: We're hiding the error message here.
    // TODO: Having the resolver type mentioned in this error causes an infinite cycle which
    // overflows Rust.
    // #[error("group error occurred")]
    // Group(GroupError<MemberId, MessageId, AuthResolver, DocumentOrderer<GS>, GS>),
}

#[cfg(test)]
mod tests {
    // ~~~~~~~~~~~
    // Group Store
    // ~~~~~~~~~~~

    use std::convert::Infallible;

    use p2panda_auth::group::resolver::GroupResolver;
    use p2panda_auth::traits::GroupStore;

    use super::{AuthGroupStateInner, MemberId, Universe};

    #[derive(Debug, Clone)]
    pub struct SqliteStore;

    impl SqliteStore {
        pub fn new() -> Self {
            Self {}
        }
    }

    impl GroupStore<MemberId, AuthGroupStateInner> for SqliteStore {
        type State = AuthGroupStateInner;

        type Error = Infallible;

        // TODO: No writes here.
        fn insert(
            y: Self::State,
            id: &MemberId,
            group: &AuthGroupStateInner,
        ) -> Result<Self::State, Self::Error> {
            todo!()
        }

        fn get(y: &Self::State, id: &MemberId) -> Result<Option<AuthGroupStateInner>, Self::Error> {
            todo!()
        }
    }

    #[test]
    fn it_works() {
        let store = SqliteStore::new();
        // let resolver = GroupResolver::default();
        let conditions = ();

        let universe = Universe::new(conditions, store);
    }
}
