// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::traits::{AckedGroupMembership, IdentityHandle, OperationId};

/// Non-optimal "Acked" Decentralised Group Membership CRDT implementation for p2panda's
/// message encryption scheme.
///
/// This does not support re-adding a member, is only used for testing and will be soon
/// replaced with an optimal implementation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AckedTestDgm<ID, OP> {
    _marker: PhantomData<(ID, OP)>,
}

impl<ID, OP> AckedTestDgm<ID, OP>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
    OP: OperationId + Serialize + for<'a> Deserialize<'a>,
{
    pub fn init(my_id: ID) -> State<ID, OP> {
        State {
            my_id,
            members: HashSet::new(),
            removed_members: HashSet::new(),
            infos: HashMap::new(),
            remove_infos: HashMap::new(),
            adds_by_msg: HashMap::new(),
            removes_by_msg: HashSet::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State<ID, OP>
where
    ID: IdentityHandle,
    OP: OperationId,
{
    my_id: ID,
    members: HashSet<ID>,
    removed_members: HashSet<ID>,
    infos: HashMap<ID, MemberInfo<ID, OP>>,
    remove_infos: HashMap<OP, RemoveInfo<ID>>,
    adds_by_msg: HashMap<OP, ID>,
    removes_by_msg: HashSet<OP>,
}

impl<ID, OP> AckedGroupMembership<ID, OP> for AckedTestDgm<ID, OP>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
    OP: OperationId + Serialize + for<'a> Deserialize<'a>,
{
    type State = State<ID, OP>;

    type Error = TestAckedGroupError<ID, OP>;

    fn from_welcome(
        mut y: Self::State,
        y_welcome: Self::State,
    ) -> Result<Self::State, Self::Error> {
        // TODO: This does not handle scenarios well when the same members get concurrently added.
        // Merge states.
        y.members.extend(y_welcome.members);
        y.removed_members.extend(y_welcome.removed_members);
        y.infos.extend(y_welcome.infos);
        y.remove_infos.extend(y_welcome.remove_infos);
        y.adds_by_msg.extend(y_welcome.adds_by_msg);
        y.removes_by_msg.extend(y_welcome.removes_by_msg);
        Ok(y)
    }

    fn create(my_id: ID, initial_members: &[ID]) -> Result<Self::State, Self::Error> {
        let mut initial_members = initial_members.to_vec();
        if !initial_members.contains(&my_id) {
            initial_members.push(my_id);
        }

        let mut infos = HashMap::with_capacity(initial_members.len());
        let mut members = HashSet::with_capacity(initial_members.len());

        for member in &initial_members {
            infos.insert(*member, MemberInfo::new(*member, None, &initial_members));
            members.insert(*member);
        }

        Ok(Self::State {
            my_id,
            members,
            removed_members: HashSet::new(),
            infos,
            remove_infos: HashMap::new(),
            adds_by_msg: HashMap::new(),
            removes_by_msg: HashSet::new(),
        })
    }

    /// Handles message adding a new member ("added") to the group by another member ("adder").
    /// Please note that a user can only be added to a group once.
    fn add(
        mut y: Self::State,
        adder: ID,
        added: ID,
        message_id: OP,
    ) -> Result<Self::State, Self::Error> {
        let mut added_info = MemberInfo::new(added, Some(adder), &[]);
        added_info.acks.insert(adder);
        added_info.acks.insert(added);
        added_info.acks.insert(y.my_id);

        // Is `actor` still a member of the group itself?
        if y.members.contains(&adder) {
            // TODO: How to handle adds when the member already exists? This de-duplicates the
            // member, but overwrites the `added_info` with a new state?
            y.members.insert(added);
            y.infos.insert(added, added_info);
        } else {
            // `actor` has been removed by a remove messages concurrent to this add message.
            // All the remove messages removing `actor` get credit for removing `added` as
            // well.
            let actor_info = y
                .infos
                .get_mut(&adder)
                .ok_or(TestAckedGroupError::UnrecognizedMember(adder))?;
            for remove_message_id in &actor_info.remove_messages {
                let remove_info = y
                    .remove_infos
                    .get_mut(remove_message_id)
                    .expect("remove_infos values should be consistent with remove_messages");
                remove_info.removed.insert(added);
                added_info.remove_messages.push(*remove_message_id);
            }
            y.removed_members.insert(added);
            y.infos.insert(added, added_info);
        }

        // If `actor` acknowledged adding or removing a member in the past, then we can be sure
        // that `added` also acknowledges it as they must have been made aware of this history
        // by receiving the welcome message from `actor`.
        for member in &y.members {
            let member_info = y
                .infos
                .get_mut(member)
                .expect("infos values should be consistent with members keys");
            if member_info.acks.contains(&adder) {
                member_info.acks.insert(added);
            }
        }

        for member in &y.removed_members {
            let member_info = y
                .infos
                .get_mut(member)
                .expect("infos values should be consistent with removed_members keys");
            if member_info.acks.contains(&adder) {
                member_info.acks.insert(added);
            }
        }

        for message_id in &y.removes_by_msg {
            let remove_info = y
                .remove_infos
                .get_mut(message_id)
                .expect("remove_infos values should be consistent with removes_by_msg keys");
            if remove_info.acks.contains(&adder) {
                remove_info.acks.insert(added);
            }
        }

        y.adds_by_msg.insert(message_id, added);

        Ok(y)
    }

    fn remove(
        mut y: Self::State,
        remover: ID,
        removed: &ID,
        message_id: OP,
    ) -> Result<Self::State, Self::Error> {
        let removed: &[ID] = &[*removed];

        let mut remove_result = Vec::new();

        let mut remove_info = RemoveInfo::new(removed);
        remove_info.acks.insert(remover);
        remove_info.acks.insert(y.my_id);

        // Remove the users in removed (if needed) and mark them as removed by this message.
        for removed_member in removed {
            let has_removed_member = y.members.remove(removed_member);
            if has_removed_member {
                let member_info = y
                    .infos
                    .get_mut(removed_member)
                    .expect("infos values should be consistent with members keys");
                y.removed_members.insert(*removed_member);
                member_info.remove_messages.push(message_id);
                remove_result.push(*removed_member);
            } else if y.removed_members.contains(removed_member) {
                if let Some(member_info) = y.infos.get_mut(removed_member) {
                    // Member has already been removed.
                    member_info.remove_messages.push(message_id);
                } else {
                    return Err(TestAckedGroupError::UnrecognizedMember(*removed_member));
                }
            } else {
                return Err(TestAckedGroupError::UnrecognizedMember(*removed_member));
            }
        }

        y.removes_by_msg.insert(message_id);
        y.remove_infos.insert(message_id, remove_info);

        // If a removed user performed an add concurrent to this message (i.e., not yet ack'd by
        // actor), then the user added by that message is also considered removed by this
        // message. This loop searches for such adds and removes their target.
        //
        // Since users removed in this fashion may themselves have added users, we have to apply
        // this rule repeatedly until it stops making progress.
        loop {
            let remove_info = y
                .remove_infos
                .get_mut(&message_id)
                .expect("infos values should be consistent with members keys");

            let mut made_progress = false;

            for member in &y.members {
                let member_info = y
                    .infos
                    .get_mut(member)
                    .expect("infos values should be consistent with members keys");
                let contains = member_info
                    .actor
                    .is_some_and(|actor| remove_info.removed.contains(&actor));
                if contains && !member_info.acks.contains(&remover) {
                    remove_result.push(*member);
                    y.removed_members.insert(*member);
                    member_info.remove_messages.push(message_id);
                    remove_info.removed.insert(*member);
                    made_progress = true;
                }
            }

            for removed_member in &remove_result {
                y.members.remove(removed_member);
            }

            // Loop through already removed users, adding this message to their list of remove
            // messages if it applies.
            for member in &y.removed_members {
                let member_info = y
                    .infos
                    .get_mut(member)
                    .expect("infos values should be consistent with members keys");
                let contains = member_info
                    .actor
                    .is_some_and(|actor| remove_info.removed.contains(&actor));
                if contains
                    && member_info.acks.contains(&remover)
                    && member_info.remove_messages.contains(&message_id)
                {
                    remove_info.removed.insert(*member);
                    made_progress = true;
                }
            }

            if !made_progress {
                break;
            }
        }

        Ok(y)
    }

    fn members_view(y: &Self::State, viewer: &ID) -> Result<HashSet<ID>, Self::Error> {
        if viewer.eq(&y.my_id) {
            return Ok(y.members.clone());
        }

        let mut view = HashSet::new();

        // Include current members whose add was acked by viewer.
        for member in &y.members {
            let member_info = y
                .infos
                .get(member)
                .expect("infos values should be consistent with members keys");
            if member_info.acks.contains(viewer) {
                view.insert(*member);
            }
        }

        // Also include removed members, none of whose removes have been acked by viewer.
        for member in &y.removed_members {
            let member_info = y
                .infos
                .get(member)
                .expect("infos values should be consistent with removed_members keys");
            let any_acked = member_info.remove_messages.iter().any(|message_id| {
                let remove_info = y
                    .remove_infos
                    .get(message_id)
                    .expect("remove_infos values should be consistent with remove_messages");
                remove_info.acks.contains(viewer)
            });
            if !any_acked {
                // Add the member to the view if the removal was not acked yet.
                //
                // It's possible that "removed_members" contains transitive removals (Charlie added
                // Bob, but Alice removed Charlie, so we'll end up with Charlie AND Bob in the
                // "removed_members" set).
                //
                // The viewer might still not have recognized that add of Bob, so we still need to
                // check if they acked the "add" itself:
                if member_info.acks.contains(viewer) {
                    view.insert(*member);
                }
            }
        }

        Ok(view)
    }

    fn ack(mut y: Self::State, acker: ID, message_id: OP) -> Result<Self::State, Self::Error> {
        let added = y.adds_by_msg.get_mut(&message_id);
        match added {
            Some(added) => {
                let member_info = y
                    .infos
                    .get_mut(added)
                    .expect("adds_by_msg values should be consistent with members keys");
                member_info.acks.insert(acker);
            }
            None => {
                let remove_info = y.remove_infos.get_mut(&message_id);
                match remove_info {
                    Some(remove_info) => {
                        if !remove_info.acks.insert(acker) {
                            return Err(TestAckedGroupError::AlreadyAcked);
                        }

                        if remove_info.removed.contains(&acker) {
                            return Err(TestAckedGroupError::AckingOwnRemoval);
                        }
                    }
                    None => return Err(TestAckedGroupError::UnknownMessage(message_id)),
                }
            }
        }

        Ok(y)
    }

    fn is_add(y: &Self::State, message_id: OP) -> bool {
        y.adds_by_msg.contains_key(&message_id)
    }

    fn is_remove(y: &Self::State, message_id: OP) -> bool {
        y.removes_by_msg.contains(&message_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MemberInfo<ID, OP>
where
    ID: IdentityHandle,
{
    pub id: ID,

    /// Who added this member.
    pub actor: Option<ID>,

    /// Remove messages that removed this member.
    pub remove_messages: Vec<OP>,

    /// Users who have ack'd the message.
    pub acks: HashSet<ID>,
}

impl<ID, OP> MemberInfo<ID, OP>
where
    ID: IdentityHandle,
    OP: OperationId,
{
    fn new(id: ID, actor: Option<ID>, initial_acks: &[ID]) -> Self {
        let mut acks = HashSet::with_capacity(initial_acks.len());
        for ack in initial_acks {
            acks.insert(*ack);
        }

        Self {
            id,
            actor,
            remove_messages: Vec::new(),
            acks,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RemoveInfo<ID>
where
    ID: IdentityHandle,
{
    /// Users removed by this message, including users who would have been removed except they were
    /// removed previously.
    pub removed: HashSet<ID>,

    /// Users who have ack'd the member.
    pub acks: HashSet<ID>,
}

impl<ID> RemoveInfo<ID>
where
    ID: IdentityHandle,
{
    pub fn new(removed_members: &[ID]) -> Self {
        let mut removed = HashSet::with_capacity(removed_members.len());
        for member in removed_members {
            removed.insert(*member);
        }

        Self {
            removed,
            acks: HashSet::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum TestAckedGroupError<ID, OP> {
    #[error("tried to access unrecognized member")]
    UnrecognizedMember(ID),

    #[error("already acked")]
    AlreadyAcked,

    #[error("member acking their own removal")]
    AckingOwnRemoval,

    #[error("message not recognized")]
    UnknownMessage(OP),
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::test_utils::MessageId;
    use crate::traits::AckedGroupMembership;

    use super::AckedTestDgm;

    #[test]
    fn concurrent_operations() {
        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        // Charlie creates a group (charlie: seq=0 "create").

        let charlie_y = AckedTestDgm::create(charlie, &[charlie]).unwrap();

        // Charlie adds Alice (charlie: seq=1 "add").

        let charlie_y = AckedTestDgm::add(
            charlie_y,
            charlie,
            alice,
            MessageId {
                sender: charlie,
                seq: 1,
            },
        )
        .unwrap();

        // Alice processes the "add" of Charlie (alice: seq=0 "ack").

        let alice_y = AckedTestDgm::init(alice);
        let alice_y = AckedTestDgm::from_welcome(alice_y, charlie_y.clone()).unwrap();

        // Charlie processes Alice's ack.

        let charlie_y = AckedTestDgm::ack(
            charlie_y,
            alice,
            MessageId {
                sender: charlie,
                seq: 1,
            },
        )
        .unwrap();

        // They have the same view on the group.

        for id in [alice, charlie] {
            assert_eq!(
                AckedTestDgm::members_view(&charlie_y, &id).unwrap(),
                HashSet::from([alice, charlie])
            );

            assert_eq!(
                AckedTestDgm::members_view(&alice_y, &id).unwrap(),
                HashSet::from([alice, charlie])
            );
        }

        // -----------------

        // Charlie adds Daphne (charlie: seq=2 "add").

        let charlie_y = AckedTestDgm::add(
            charlie_y,
            charlie,
            daphne,
            MessageId {
                sender: charlie,
                seq: 2,
            },
        )
        .unwrap();

        assert_eq!(
            AckedTestDgm::members_view(&charlie_y, &charlie).unwrap(),
            HashSet::from([alice, charlie, daphne])
        );

        // Daphne processes their add (daphne: seq=0 "ack").

        let daphne_y = AckedTestDgm::init(daphne);
        let daphne_y = AckedTestDgm::from_welcome(daphne_y, charlie_y.clone()).unwrap();

        assert_eq!(
            AckedTestDgm::members_view(&daphne_y, &daphne).unwrap(),
            HashSet::from([alice, charlie, daphne])
        );

        // Alice processes Charlie's "add" of Daphne (alice: seq=1 "ack").

        let alice_y = AckedTestDgm::add(
            alice_y,
            charlie,
            daphne,
            MessageId {
                sender: charlie,
                seq: 2,
            },
        )
        .unwrap();

        assert_eq!(
            AckedTestDgm::members_view(&alice_y, &alice).unwrap(),
            HashSet::from([alice, charlie, daphne])
        );

        // Everyone processes each other's acks.

        let charlie_y = AckedTestDgm::ack(
            charlie_y,
            daphne,
            MessageId {
                sender: charlie,
                seq: 2,
            },
        )
        .unwrap();
        let alice_y = AckedTestDgm::ack(
            alice_y,
            daphne,
            MessageId {
                sender: charlie,
                seq: 2,
            },
        )
        .unwrap();
        let charlie_y = AckedTestDgm::ack(
            charlie_y,
            alice,
            MessageId {
                sender: charlie,
                seq: 2,
            },
        )
        .unwrap();
        let daphne_y = AckedTestDgm::ack(
            daphne_y,
            alice,
            MessageId {
                sender: charlie,
                seq: 2,
            },
        )
        .unwrap();

        // Everyone should have the same members views.

        for id in [alice, charlie, daphne] {
            assert_eq!(
                AckedTestDgm::members_view(&alice_y, &id).unwrap(),
                HashSet::from([alice, charlie, daphne])
            );

            assert_eq!(
                AckedTestDgm::members_view(&charlie_y, &id).unwrap(),
                HashSet::from([alice, charlie, daphne])
            );

            assert_eq!(
                AckedTestDgm::members_view(&daphne_y, &id).unwrap(),
                HashSet::from([alice, charlie, daphne])
            );
        }

        // ----------------------

        // Alice removes Charlie (alice: seq=2 "remove").

        let alice_y = AckedTestDgm::remove(
            alice_y,
            alice,
            &charlie,
            MessageId {
                sender: alice,
                seq: 2,
            },
        )
        .unwrap();

        assert_eq!(
            AckedTestDgm::members_view(&alice_y, &alice).unwrap(),
            HashSet::from([alice, daphne])
        );

        // Charlie adds Bob concurrently (charlie: seq=3 "add").

        let charlie_y = AckedTestDgm::add(
            charlie_y,
            charlie,
            bob,
            MessageId {
                sender: charlie,
                seq: 3,
            },
        )
        .unwrap();

        for id in [bob, charlie] {
            assert_eq!(
                AckedTestDgm::members_view(&charlie_y, &id).unwrap(),
                HashSet::from([alice, bob, charlie, daphne])
            );
        }

        for id in [alice, daphne] {
            assert_eq!(
                AckedTestDgm::members_view(&charlie_y, &id).unwrap(),
                HashSet::from([alice, charlie, daphne])
            );
        }

        // Bob processes their addition.

        let bob_y = AckedTestDgm::init(bob);
        let bob_y = AckedTestDgm::from_welcome(bob_y, charlie_y.clone()).unwrap();

        for id in [bob, charlie] {
            assert_eq!(
                AckedTestDgm::members_view(&bob_y, &id).unwrap(),
                HashSet::from([alice, bob, charlie, daphne])
            );
        }

        // Everyone processes the removal of Charlie.

        let bob_y = AckedTestDgm::remove(
            bob_y,
            alice,
            &charlie,
            MessageId {
                sender: alice,
                seq: 2,
            },
        )
        .unwrap();

        let charlie_y = AckedTestDgm::remove(
            charlie_y,
            alice,
            &charlie,
            MessageId {
                sender: alice,
                seq: 2,
            },
        )
        .unwrap();

        let daphne_y = AckedTestDgm::remove(
            daphne_y,
            alice,
            &charlie,
            MessageId {
                sender: alice,
                seq: 2,
            },
        )
        .unwrap();

        assert_eq!(
            AckedTestDgm::members_view(&bob_y, &bob).unwrap(),
            HashSet::from([alice, daphne])
        );

        assert_eq!(
            AckedTestDgm::members_view(&charlie_y, &charlie).unwrap(),
            HashSet::from([alice, daphne])
        );

        assert_eq!(
            AckedTestDgm::members_view(&daphne_y, &daphne).unwrap(),
            HashSet::from([alice, daphne])
        );

        // Everyone else processes the add of Bob.

        let alice_y = AckedTestDgm::add(
            alice_y,
            charlie,
            bob,
            MessageId {
                sender: charlie,
                seq: 3,
            },
        )
        .unwrap();

        let daphne_y = AckedTestDgm::add(
            daphne_y,
            charlie,
            bob,
            MessageId {
                sender: charlie,
                seq: 3,
            },
        )
        .unwrap();

        // Because of the strong removal CRDT, Bob's add will not be recognized as the adder
        // Charlie got removed by Alice.

        assert_eq!(
            AckedTestDgm::members_view(&alice_y, &alice).unwrap(),
            HashSet::from([alice, daphne])
        );

        assert_eq!(
            AckedTestDgm::members_view(&charlie_y, &charlie).unwrap(),
            HashSet::from([alice, daphne])
        );

        assert_eq!(
            AckedTestDgm::members_view(&daphne_y, &daphne).unwrap(),
            HashSet::from([alice, daphne])
        );

        // Since nothing was ack'ed yet, all members should believe that the other's didn't do any
        // changes to their member views yet.

        assert_eq!(
            AckedTestDgm::members_view(&alice_y, &daphne).unwrap(),
            HashSet::from([alice, charlie, daphne])
        );

        assert_eq!(
            AckedTestDgm::members_view(&bob_y, &alice).unwrap(),
            HashSet::from([alice, daphne])
        );

        assert_eq!(
            AckedTestDgm::members_view(&bob_y, &charlie).unwrap(),
            HashSet::from([alice, bob, charlie, daphne])
        );

        assert_eq!(
            AckedTestDgm::members_view(&bob_y, &daphne).unwrap(),
            HashSet::from([alice, charlie, daphne])
        );

        assert_eq!(
            AckedTestDgm::members_view(&charlie_y, &daphne).unwrap(),
            HashSet::from([alice, charlie, daphne])
        );
    }

    #[test]
    fn members_view() {
        let alice = 0;
        let bob = 1;
        let charlie = 2;

        // Alice creates a group with Charlie.

        let alice_y = AckedTestDgm::create(alice, &[alice, charlie]).unwrap();

        let charlie_y = AckedTestDgm::init(charlie);
        let charlie_y = AckedTestDgm::from_welcome(charlie_y, alice_y.clone()).unwrap();

        // Alice adds Bob.

        let alice_y = AckedTestDgm::add(
            alice_y,
            alice,
            bob,
            MessageId {
                sender: alice,
                seq: 1,
            },
        )
        .unwrap();

        let bob_y = AckedTestDgm::init(bob);
        let bob_y = AckedTestDgm::from_welcome(bob_y, alice_y.clone()).unwrap();

        // Bob acks their own add, Charlie doesn't ack yet.

        let alice_y = AckedTestDgm::ack(
            alice_y,
            bob,
            MessageId {
                sender: alice,
                seq: 1,
            },
        )
        .unwrap();

        // Both Alice and Bob consider all three members of the set.

        assert_eq!(
            AckedTestDgm::members_view(&alice_y, &alice).unwrap(),
            HashSet::from([alice, bob, charlie])
        );

        assert_eq!(
            AckedTestDgm::members_view(&alice_y, &bob).unwrap(),
            HashSet::from([alice, bob, charlie])
        );

        assert_eq!(
            AckedTestDgm::members_view(&bob_y, &alice).unwrap(),
            HashSet::from([alice, bob, charlie])
        );

        assert_eq!(
            AckedTestDgm::members_view(&bob_y, &bob).unwrap(),
            HashSet::from([alice, bob, charlie])
        );

        // Charlie didn't ack added Bob yet.

        assert_eq!(
            AckedTestDgm::members_view(&alice_y, &charlie).unwrap(),
            HashSet::from([alice, charlie])
        );

        assert_eq!(
            AckedTestDgm::members_view(&bob_y, &charlie).unwrap(),
            HashSet::from([alice, charlie])
        );

        assert_eq!(
            AckedTestDgm::members_view(&charlie_y, &charlie).unwrap(),
            HashSet::from([alice, charlie])
        );

        // Charlie processes and acks added Bob.

        let charlie_y = AckedTestDgm::add(
            charlie_y,
            alice,
            bob,
            MessageId {
                sender: alice,
                seq: 1,
            },
        )
        .unwrap();

        let alice_y = AckedTestDgm::ack(
            alice_y,
            charlie,
            MessageId {
                sender: alice,
                seq: 1,
            },
        )
        .unwrap();

        let bob_y = AckedTestDgm::ack(
            bob_y,
            charlie,
            MessageId {
                sender: alice,
                seq: 1,
            },
        )
        .unwrap();

        // Everyone should have the same view.

        for id in [alice, bob, charlie] {
            assert_eq!(
                AckedTestDgm::members_view(&alice_y, &id).unwrap(),
                HashSet::from([alice, bob, charlie])
            );

            assert_eq!(
                AckedTestDgm::members_view(&bob_y, &id).unwrap(),
                HashSet::from([alice, bob, charlie])
            );

            assert_eq!(
                AckedTestDgm::members_view(&charlie_y, &id).unwrap(),
                HashSet::from([alice, bob, charlie])
            );
        }

        // Charlie removes Bob.

        let charlie_y = AckedTestDgm::remove(
            charlie_y,
            charlie,
            &bob,
            MessageId {
                sender: charlie,
                seq: 2,
            },
        )
        .unwrap();

        // Alice and Bob process the removal.

        let alice_y = AckedTestDgm::remove(
            alice_y,
            charlie,
            &bob,
            MessageId {
                sender: charlie,
                seq: 2,
            },
        )
        .unwrap();

        let bob_y = AckedTestDgm::remove(
            bob_y,
            charlie,
            &bob,
            MessageId {
                sender: charlie,
                seq: 2,
            },
        )
        .unwrap();

        // Everyone considers for themselves and for Charlie (the "remover") that Bob is removed
        // from the group.

        assert_eq!(
            AckedTestDgm::members_view(&charlie_y, &charlie).unwrap(),
            HashSet::from([alice, charlie])
        );

        for id in [alice, charlie] {
            assert_eq!(
                AckedTestDgm::members_view(&alice_y, &id).unwrap(),
                HashSet::from([alice, charlie])
            );
        }

        for id in [bob, charlie] {
            assert_eq!(
                AckedTestDgm::members_view(&bob_y, &id).unwrap(),
                HashSet::from([alice, charlie])
            );
        }

        // .. but they assume so far that the other's still consider Bob part of the group (because
        // no acks have been observed yet).

        assert_eq!(
            AckedTestDgm::members_view(&alice_y, &bob).unwrap(),
            HashSet::from([alice, bob, charlie]),
        );

        assert_eq!(
            AckedTestDgm::members_view(&bob_y, &alice).unwrap(),
            HashSet::from([alice, bob, charlie]),
        );

        for id in [alice, bob] {
            assert_eq!(
                AckedTestDgm::members_view(&charlie_y, &id).unwrap(),
                HashSet::from([alice, bob, charlie]),
                "invalid members view from 2's perspective for {id}",
            );
        }
    }

    #[test]
    fn strong_removal() {
        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        // Alice creates a group.

        let alice_y = AckedTestDgm::create(alice, &[alice]).unwrap();

        // Alice adds Bob.

        let alice_y = AckedTestDgm::add(
            alice_y,
            alice,
            bob,
            MessageId {
                sender: alice,
                seq: 0,
            },
        )
        .unwrap();

        let bob_y = AckedTestDgm::init(bob);
        let bob_y = AckedTestDgm::from_welcome(bob_y, alice_y.clone()).unwrap();

        // Alice removes Bob.

        let alice_y = AckedTestDgm::remove(
            alice_y,
            alice,
            &bob,
            MessageId {
                sender: alice,
                seq: 0,
            },
        )
        .unwrap();

        // Concurrently Bob adds Charlie and Daphne.

        let bob_y = AckedTestDgm::add(
            bob_y,
            bob,
            charlie,
            MessageId {
                sender: bob,
                seq: 0,
            },
        )
        .unwrap();

        let bob_y = AckedTestDgm::add(
            bob_y,
            bob,
            daphne,
            MessageId {
                sender: bob,
                seq: 1,
            },
        )
        .unwrap();

        // Alice applies Bob's changes.

        let alice_y = AckedTestDgm::add(
            alice_y,
            bob,
            charlie,
            MessageId {
                sender: bob,
                seq: 0,
            },
        )
        .unwrap();

        let alice_y = AckedTestDgm::add(
            alice_y,
            bob,
            daphne,
            MessageId {
                sender: bob,
                seq: 1,
            },
        )
        .unwrap();

        // Bob applies Alice's changes.

        let bob_y = AckedTestDgm::remove(
            bob_y,
            alice,
            &bob,
            MessageId {
                sender: alice,
                seq: 1,
            },
        )
        .unwrap();

        assert_eq!(
            AckedTestDgm::members_view(&alice_y, &alice).unwrap(),
            AckedTestDgm::members_view(&bob_y, &bob).unwrap(),
        );
        assert!(
            !AckedTestDgm::members_view(&alice_y, &alice)
                .unwrap()
                .contains(&bob)
        );
        assert!(
            !AckedTestDgm::members_view(&alice_y, &alice)
                .unwrap()
                .contains(&charlie)
        );
        assert!(
            !AckedTestDgm::members_view(&alice_y, &alice)
                .unwrap()
                .contains(&daphne)
        );
    }
}
