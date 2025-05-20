use std::collections::HashMap;
use std::marker::PhantomData;

use p2panda_auth::group::test_utils::{TestGroupState, TestGroupStateInner, TestGroupStoreState};
use p2panda_auth::group::{
    Group, GroupAction, GroupControlMessage, GroupError, GroupMember, GroupState,
};
use p2panda_auth::group_crdt::Access;
use p2panda_auth::traits::Ordering as AuthOrdering;
use p2panda_core::{Hash, PublicKey};
use p2panda_encryption::KeyRegistry;
use thiserror::Error;

pub type GroupId = PublicKey;

pub type MemberId = PublicKey;

pub struct DocumentOrderer {}

#[derive(Clone, Debug)]
pub struct DocumentOrdererState {}

struct INTERESTING {}

impl AuthOrdering<PublicKey, Hash, GroupControlMessage<PublicKey, Hash>> for DocumentOrderer {
    type State = DocumentOrdererState;

    type Message = INTERESTING;

    type Error = DocumentError;

    fn next_message(
        y: Self::State,
        control_message: &GroupControlMessage<PublicKey, Hash>,
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

pub type AuthGroup<RS, S> = Group<PublicKey, Hash, RS, DocumentOrderer, S>;

pub struct Document<C> {
    group_id: PublicKey,
    // auth_state: TestGroupState,
    // auth_group_store: TestGroupStoreState<MemberId, TestGroupStateInner>,
    _marker: PhantomData<C>,
}

impl<C> Document<C> {
    pub fn create(
        my_id: PublicKey,
        initial_members: &[(GroupMember<PublicKey>, Access<C>)],
    ) -> Self {
        // TODO: Here something happens with deriving a group id.
        todo!()
    }

    // We call this after receiving a CREATE or ADD which brings us into a document.
    pub fn from_welcome(my_id: MemberId, group_id: GroupId) -> Self {
        todo!()
        // Self {
        //     auth_state: TestGroupState::new(my_id, group_id, group_store_y, orderer_y),
        //     auth_group_store: TestGroupStoreState::default(),
        // }
    }

    // TODO: Access should use our C generic here.
    pub fn add(&self, added: GroupMember<PublicKey>, access: Access<()>) {
        let control_message = GroupControlMessage::GroupAction {
            group_id: self.group_id,
            action: GroupAction::Add {
                member: added,
                access,
            },
        };

        let (group_y, operation_001) = TestGroup::prepare(group_y, &control_message_001).unwrap();
        let group_y = TestGroup::process(group_y, &operation_001).unwrap();
    }
}

enum UniverseMessage {
    KeyBundle,
    Group,
    Document,
}

// "App Universe"
struct Universe<C> {
    // Here we have _all_ groups EXCEPT "root groups" / documents.
    groups: HashMap<PublicKey, GroupState>,

    // Here we have all "root groups" / documents, no "sub groups".
    documents: HashMap<PublicKey, Document<C>>,

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

    let document = Document::new();
}

#[derive(Debug, Error)]
enum DocumentError {}
