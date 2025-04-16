// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils {
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
    pub struct AckedTestDGM<ID, OP> {
        _marker: PhantomData<(ID, OP)>,
    }

    impl<ID, OP> AckedTestDGM<ID, OP>
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

    impl<ID, OP> AckedGroupMembership<ID, OP> for AckedTestDGM<ID, OP>
    where
        ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
        OP: OperationId + Serialize + for<'a> Deserialize<'a>,
    {
        type State = State<ID, OP>;

        type Error = TestAckedGroupError<ID, OP>;

        fn from_welcome(my_id: ID, mut y: Self::State) -> Result<Self::State, Self::Error> {
            y.my_id = my_id;
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
                    view.insert(*member);
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
                    if !member_info.acks.insert(acker) && !member_info.id.eq(&acker) {
                        // TODO: This is weird.
                        // Don't complain if its the added user acking themselves (for real this
                        // time, as opposed to the implicit ack that they give just from being
                        // added).
                        // return Err(TestAckedGroupError::AlreadyAcked);
                    }
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
        use crate::test_utils::MessageId;
        use crate::traits::AckedGroupMembership;

        use super::AckedTestDGM;

        #[test]
        fn concurrent_operations() {
            let alice = 0;
            let bob = 1;
            let charlie = 2;
            let daphne = 3;

            // Alice creates a group.
            let alice_y = AckedTestDGM::create(alice, &[alice]).unwrap();

            // Alice adds Bob.
            let alice_y = AckedTestDGM::add(
                alice_y,
                alice,
                bob,
                MessageId {
                    sender: alice,
                    seq: 0,
                },
            )
            .unwrap();
            let bob_y = AckedTestDGM::from_welcome(bob, alice_y.clone()).unwrap();

            // Alice removes Bob.
            let alice_y = AckedTestDGM::remove(
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
            let bob_y = AckedTestDGM::add(
                bob_y,
                bob,
                charlie,
                MessageId {
                    sender: bob,
                    seq: 0,
                },
            )
            .unwrap();

            let bob_y = AckedTestDGM::add(
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
            let alice_y = AckedTestDGM::add(
                alice_y,
                bob,
                charlie,
                MessageId {
                    sender: bob,
                    seq: 0,
                },
            )
            .unwrap();

            let alice_y = AckedTestDGM::add(
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
            let bob_y = AckedTestDGM::remove(
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
                AckedTestDGM::members_view(&alice_y, &alice).unwrap(),
                AckedTestDGM::members_view(&bob_y, &bob).unwrap(),
            );
            assert!(
                !AckedTestDGM::members_view(&alice_y, &alice)
                    .unwrap()
                    .contains(&bob)
            );
            assert!(
                !AckedTestDGM::members_view(&alice_y, &alice)
                    .unwrap()
                    .contains(&charlie)
            );
            assert!(
                !AckedTestDGM::members_view(&alice_y, &alice)
                    .unwrap()
                    .contains(&daphne)
            );
        }
    }
}
