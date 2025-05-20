use std::collections::HashMap;
use std::fmt::{self, Display};
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use p2panda_auth::group::{
    Group, GroupAction, GroupControlMessage, GroupMember, GroupState, GroupStateInner,
};
use p2panda_auth::group_crdt::Access;
use p2panda_auth::traits::{
    AuthGraph, GroupStore, IdentityHandle, Operation as AuthOperation, OperationId,
};
use p2panda_auth::traits::{Ordering as AuthOrdering, Resolver};
use p2panda_core::{Hash, PublicKey};
use p2panda_encryption::KeyRegistry;
use thiserror::Error;

// ~~~~~~~~~~
// Core types
// ~~~~~~~~~~

// Can be both a group id or individual id.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, StdHash)]
pub struct MemberId(pub PublicKey);

impl Display for MemberId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl IdentityHandle for MemberId {}

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

pub type AuthGroup<RS, GS> = Group<MemberId, MessageId, RS, DocumentOrderer, GS>;

pub type AuthGroupState<RS, GS> = GroupState<MemberId, MessageId, RS, DocumentOrderer, GS>;

// TODO: This will probably be removed soon?
pub type AuthGroupStateInner = GroupStateInner<MemberId, MessageId, DocumentMessage>;

pub type AuthControlMessage = GroupControlMessage<MemberId, MessageId>;

// ~~~~~~~
// Orderer
// ~~~~~~~

#[derive(Clone, Debug)]
pub struct DocumentOrderer {}

#[derive(Clone, Debug)]
pub struct DocumentOrdererState {}

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

impl AuthOrdering<MemberId, MessageId, AuthControlMessage> for DocumentOrderer {
    type State = DocumentOrdererState;

    type Message = DocumentMessage;

    type Error = DocumentError;

    fn next_message(
        y: Self::State,
        control_message: &AuthControlMessage,
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

// ~~~~~~~~
// Document
// ~~~~~~~~

pub struct Document<C, RS, GS>
where
    RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage>,
    GS: GroupStore<MemberId, AuthGroupStateInner>,
{
    group_id: MemberId,
    auth_state: AuthGroupState<RS, GS>,
    auth_store: GS,
    _marker: PhantomData<C>,
}

impl<C, RS, GS> Document<C, RS, GS>
where
    RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage>,
    GS: GroupStore<MemberId, AuthGroupStateInner>,
{
    pub fn create(
        my_id: PublicKey,
        initial_members: &[(GroupMember<MemberId>, Access<C>)],
    ) -> Self {
        // TODO: Here something happens with deriving a group id.
        todo!()
    }

    // We call this after receiving a CREATE or ADD which brings us into a document.
    pub fn from_welcome(my_id: MemberId, group_id: PublicKey) -> Self {
        todo!()
    }

    // TODO: Access should use our C generic here.
    pub fn add(&self, added: GroupMember<MemberId>, access: Access<()>) {
        let control_message = AuthControlMessage::GroupAction {
            group_id: self.group_id,
            action: GroupAction::Add {
                member: added,
                access,
            },
        };

        // TODO
        // let (group_y, operation_001) = AuthGroup::prepare(group_y, &control_message).unwrap();
        // let group_y = AuthGroup::process(group_y, &operation_001).unwrap();
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
struct Universe<C, RS, GS>
where
    RS: Resolver<AuthGroupState<RS, GS>, DocumentMessage>,
    GS: GroupStore<MemberId, AuthGroupStateInner>,
{
    // Here we have _all_ groups EXCEPT "root groups" / documents.
    groups: HashMap<PublicKey, AuthGroupState<RS, GS>>,

    // Here we have all "root groups" / documents, no "sub groups".
    documents: HashMap<PublicKey, Document<C, RS, GS>>,

    // Key bundles.
    key_registry: KeyRegistry<MemberId>,
}

pub fn do_it() {
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

    // let document = Document::new();
}

#[derive(Debug, Error)]
pub enum DocumentError {}
