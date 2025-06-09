// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::collections::{HashMap, VecDeque};
use std::fmt::Display;

use libfuzzer_sys::fuzz_target;
use p2panda_auth::group::test_utils::{
    MemberId, MessageId, TestGroup, TestGroupError, TestGroupState, TestGroupStore, TestOperation,
    TestOrdererState,
};
use p2panda_auth::group::{Access, GroupAction, GroupControlMessage, GroupMember};
use p2panda_auth::traits::{AuthGroup, Operation as OperationTrait};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const MEMBERS: [char; 26] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
];

const MAX_ACTION_ROUNDS: usize = 12;

const MAX_CONCURRENCY_DEPTH: u8 = 12;

const ACCESS_LEVELS: [Access<()>; 4] = [
    Access::Pull,
    Access::Read,
    Access::Write { conditions: None },
    Access::Manage,
];

fn random_u8(rng: &mut StdRng) -> u8 {
    let value: [u8; 1] = rng.random();
    value[0]
}

fn random_range(min: u8, max: u8, rng: &mut StdRng) -> u8 {
    let value = random_u8(rng);
    min + (value % (max - min + 1))
}

fn random_item<T: Clone>(vec: Vec<T>, rng: &mut StdRng) -> Option<T> {
    if vec.is_empty() {
        None
    } else {
        let random_index = random_range(0, vec.len() as u8 - 1, rng) as usize;
        Some(vec.get(random_index).cloned().unwrap())
    }
}

fn print_members(members: &[(GroupMember<MemberId>, Access<()>)]) -> String {
    members
        .iter()
        .map(|(id, access)| format!("{:?} {}", id, access))
        .collect::<Vec<String>>()
        .join(", ")
}

#[derive(Debug, PartialEq, Eq)]
enum Options {
    Add,
    Promote,
    Demote,
    Remove,
    Noop,
}

#[derive(Clone, Debug)]
enum Suggestion {
    Valid(TestGroupAction),

    #[allow(dead_code)]
    Invalid(TestGroupAction),
}

impl<'a> Suggestion {
    fn operation(&'a self) -> &'a TestGroupAction {
        match self {
            Suggestion::Valid(operation) => operation,
            Suggestion::Invalid(operation) => operation,
        }
    }
}

#[derive(Clone, Debug)]
enum TestGroupAction {
    Noop,
    Action(GroupAction<MemberId, ()>),
}

impl Display for TestGroupAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TestGroupAction::Noop => "noop".to_string(),
                TestGroupAction::Action(action) => {
                    match action {
                        GroupAction::Create { initial_members } => format!(
                            "create (initial_members={{{}}})",
                            print_members(initial_members)
                        ),
                        GroupAction::Add { member, .. } => {
                            format!("add {:?}", member,)
                        }
                        GroupAction::Remove { member } => {
                            format!("remove {:?}", member)
                        }
                        GroupAction::Promote { member, .. } => format!("promote {:?}", member),
                        GroupAction::Demote { member, .. } => format!("demote {:?}", member),
                    }
                }
            }
        )
    }
}

#[derive(Debug)]
struct Member {
    members: Vec<GroupMember<MemberId>>,
    my_id: MemberId,
    group: Option<TestGroupState>,
}

impl Member {
    pub fn id(&self) -> MemberId {
        self.my_id
    }

    pub fn is_active(&self) -> bool {
        self.group
            .clone()
            .expect("group state exists")
            .members()
            .iter()
            .any(|(member, _)| member.id() == self.id())
    }

    /// Apply and process a local group membership operation for this member.
    ///
    /// This might yield a message which then needs to be broadcast to the group.
    pub fn process_local(
        &mut self,
        operation: &TestGroupAction,
    ) -> Result<Option<TestOperation<MemberId, MessageId, ()>>, TestGroupError> {
        let y_group = self.group.take().expect("group state exists");

        // Apply and process the local operation.
        let result = match operation {
            TestGroupAction::Noop => {
                // Do nothing
                Ok((y_group, None))
            }
            TestGroupAction::Action(action) => {
                let group_operation = GroupControlMessage::GroupAction {
                    group_id: y_group.group_id,
                    action: action.clone(),
                };
                let (y_group, message) = TestGroup::prepare(y_group, &group_operation)?;
                let y_group = TestGroup::process(y_group, &message)?;

                Ok((y_group, Some(message)))
            }
        };

        match result {
            Ok((y_group_i, message)) => {
                self.group.replace(y_group_i);
                Ok(message)
            }
            Err(err) => Err(err),
        }
    }

    /// Apply and process a remote group membership operation for this member.
    ///
    /// This might yield a message which then needs to be broadcast to the group.
    pub fn process_remote(
        &mut self,
        operation: &TestOperation<MemberId, MessageId, ()>,
    ) -> Result<(), TestGroupError> {
        // Process remote operation.
        let y_group = self.group.take().expect("group state exists");
        let y_group_i = TestGroup::process(y_group, operation)?;
        self.group.replace(y_group_i);
        Ok(())
    }

    pub fn assert_state(&self, other: &Member) {
        let y_group = other.group.as_ref().expect("group state exists");
        let mut other_members = y_group.members();
        other_members.sort();

        let y_group = self.group.as_ref().expect("group state exists");
        let mut members = y_group.members();
        members.sort();

        assert_eq!(
            members,
            other_members,
            "member set of {} compared to {} ",
            self.id(),
            other.id(),
        );
    }

    fn random_active_member(
        &self,
        rng: &mut StdRng,
    ) -> Option<(GroupMember<MemberId>, Access<()>)> {
        let active_members = self.group.clone().expect("group state exists").members();
        random_item(active_members, rng)
    }

    fn random_inactive_member(&self, rng: &mut StdRng) -> Option<GroupMember<MemberId>> {
        let active_members = self.group.clone().expect("group state exists").members();
        let inactive_members = self
            .members
            .clone()
            .into_iter()
            .filter(|member| {
                !active_members
                    .iter()
                    .any(|(active_member, _)| active_member == member)
            })
            .collect();
        random_item(inactive_members, rng)
    }

    /// Suggest the next group membership operation based on the current member's state.
    pub fn suggest(&self, rng: &mut StdRng) -> Suggestion {
        let operation = if self.is_active() {
            self.suggest_valid(
                &[
                    Options::Add,
                    Options::Remove,
                    Options::Promote,
                    Options::Demote,
                    Options::Noop,
                ],
                rng,
            )
        } else {
            TestGroupAction::Noop
        };
        Suggestion::Valid(operation)
    }

    /// Randomly suggest a valid, next group operation based on a set of given options and this
    /// members' current access level (only members with manage access can perform group actions).
    fn suggest_valid(&self, try_options: &[Options], rng: &mut StdRng) -> TestGroupAction {
        let mut options = Vec::new();

        let (_, access) = self
            .group
            .clone()
            .expect("group exists")
            .members()
            .into_iter()
            .find(|(id, _)| *id == GroupMember::Individual(self.my_id))
            .expect("active member should be found");

        if access < Access::Manage {
            return TestGroupAction::Noop;
        }

        if try_options.contains(&Options::Add) {
            if let Some(member) = self.random_inactive_member(rng) {
                if member.id() != self.my_id {
                    options.push(TestGroupAction::Action(GroupAction::Add {
                        member,
                        access: random_item(ACCESS_LEVELS.to_vec(), rng).unwrap(),
                    }))
                }
            }
        }

        if try_options.contains(&Options::Promote) {
            if let Some((member, access)) = self.random_active_member(rng) {
                loop {
                    if access == Access::Manage {
                        break;
                    }

                    let next_access = random_item(ACCESS_LEVELS.to_vec(), rng).unwrap();

                    if access > next_access {
                        continue;
                    }

                    options.push(TestGroupAction::Action(GroupAction::Promote {
                        member,
                        access: next_access,
                    }));
                    break;
                }
            }
        }

        if try_options.contains(&Options::Demote) {
            if let Some((member, access)) = self.random_active_member(rng) {
                loop {
                    if access == Access::Pull {
                        break;
                    }

                    let next_access = random_item(ACCESS_LEVELS.to_vec(), rng).unwrap();

                    if access < next_access {
                        continue;
                    }

                    options.push(TestGroupAction::Action(GroupAction::Demote {
                        member,
                        access: next_access,
                    }));
                    break;
                }
            }
        }

        if try_options.contains(&Options::Remove) {
            if let Some(removed) = self.random_active_member(rng) {
                options.push(TestGroupAction::Action(GroupAction::Remove {
                    member: removed.0,
                }));
            }
        }

        if try_options.contains(&Options::Noop) {
            options.push(TestGroupAction::Noop);
        }

        match random_item(options, rng) {
            Some(operation) => operation,
            None => TestGroupAction::Noop,
        }
    }
}

fuzz_target!(|seed: [u8; 32]| {
    let mut rng = StdRng::from_seed(seed);

    // Generate a list of all members.
    let mut members: HashMap<MemberId, Member> = HashMap::new();
    let range: u8 = random_range(1, MEMBERS.len() as u8, &mut rng);
    let member_ids = MEMBERS[0..range as usize].to_vec();

    let group_size = random_range(1, MEMBERS.len() as u8, &mut rng);
    let mut initial_member_ids = MEMBERS.to_vec();
    let _ = initial_member_ids.split_off(group_size as usize);

    // Pick a random group creator.
    let group_creator = random_item(initial_member_ids.clone(), &mut rng).unwrap();

    let initial_members: Vec<(GroupMember<MemberId>, Access<()>)> = initial_member_ids
        .into_iter()
        .map(|id| {
            if id == group_creator {
                (GroupMember::Individual(id), Access::Manage)
            } else {
                (
                    GroupMember::Individual(id),
                    random_item(ACCESS_LEVELS.to_vec(), &mut rng).unwrap(),
                )
            }
        })
        .collect();

    let group_states = {
        let members = member_ids.clone();
        let mut states = HashMap::new();
        for member in members {
            let store = TestGroupStore::default();
            states.insert(
                member,
                TestGroupState::new(
                    member,
                    group_creator,
                    store.clone(),
                    TestOrdererState::new(member, store.clone(), StdRng::from_rng(&mut rng)),
                ),
            );
        }
        states
    };

    let mut queue = VecDeque::new();

    for id in &member_ids {
        members.insert(
            *id,
            Member {
                my_id: *id,
                members: member_ids
                    .iter()
                    .map(|id| GroupMember::Individual(*id))
                    .collect(),
                // Set up group state for each member.
                group: {
                    if id == &group_creator {
                        // The group "creator" initialises the group with themselves ..
                        let control_message = GroupControlMessage::GroupAction {
                            group_id: group_creator,
                            action: GroupAction::Create {
                                initial_members: initial_members.clone(),
                            },
                        };
                        let y_group = group_states[id].clone();
                        let (y_group_i, operation) =
                            TestGroup::prepare(y_group, &control_message).unwrap();
                        let y_group_ii = TestGroup::process(y_group_i, &operation).unwrap();

                        // .. and publishes the first "create" control message on the test network.
                        queue.push_back((
                            Suggestion::Valid(TestGroupAction::Action(GroupAction::Create {
                                initial_members: initial_members.clone(),
                            })),
                            operation,
                        ));

                        Some(y_group_ii)
                    } else {
                        Some(group_states[id].clone())
                    }
                },
            },
        );
    }

    drop(group_states);

    // Based on our deterministic state machines we can now generate `n` group operations for each
    // member and test the integrity and robustness of the group by processing these suggested
    // operations and comparing the resulting group state with the expected values from the state
    // machine.

    println!("\n==============================");
    println!("group created [group_creator={}]", group_creator);
    println!("==============================");

    for i in 0..MAX_ACTION_ROUNDS {
        println!("ROUND: {i}");
        // 1. Go through all members of the group, suggest and apply a local operation this member
        //    can do. Inactive or removed members will not cause any actions.

        let mut operations: Vec<(TestGroupAction, TestOperation<MemberId, MessageId, ()>)> =
            Vec::new();

        for member_id in &member_ids {
            let member = members.get_mut(member_id).expect("member exists");

            // Suggest the next group membership operation for this member.
            for _ in 0..random_range(0, MAX_CONCURRENCY_DEPTH, &mut rng) {
                let suggestion = member.suggest(&mut rng);

                if !matches!(suggestion.operation(), TestGroupAction::Noop) {
                    println!(
                        "member: {}, operation: {}",
                        member.my_id,
                        suggestion.operation(),
                    );
                }

                // Process group operation locally for this member.
                match &suggestion {
                    Suggestion::Valid(operation) => {
                        if let Some(message) =
                            member.process_local(operation).unwrap_or_else(|_| {
                                panic!("valid operations to not fail: {}", operation)
                            })
                        {
                            operations.push((operation.clone(), message.clone()));
                            queue.push_back((suggestion, message));
                        }
                    }
                    Suggestion::Invalid(operation) => {
                        assert!(
                            member.process_local(operation).is_err(),
                            "expected error due to invalid group operation"
                        );
                    }
                }
            }
        }

        while let Some((suggestion, message)) = queue.pop_front() {
            for member_id in &member_ids {
                // Do not process our own messages.
                if member_id == &message.sender() {
                    continue;
                }

                let member = members.get_mut(member_id).expect("member exists");

                //                 if let GroupControlMessage::GroupAction {
                //                     group_id,
                //                     action: GroupAction::Add { member, .. }
                //                 } = message.payload
                //                 {
                //                     if member.id() == *member_id {
                //                         member.queue
                //
                //                     }
                //                 }
                //
                //                 if !member.is_active() {
                //                     member.queue.push(message.clone());
                //                 } else {
                match member.process_remote(&message) {
                    Ok(_) => {
                        if let Suggestion::Invalid(operation) = suggestion {
                            panic!(
                                "expected error when processing remote message from invalid operation '{}'",
                                operation
                            )
                        }
                    }
                    Err(err) => {
                        if let Suggestion::Valid(operation) = suggestion {
                            panic!(
                                "unexpected error when processing remote message from valid operation member={} '{}':\n{}",
                                member.id(),
                                operation,
                                err
                            )
                        }
                    } // }
                }
            }
        }

        let active_members: Vec<(&char, &Member)> = members
            .iter()
            .filter(|(_, member)| member.is_active())
            .collect();

        if let Some((_, control_member)) = active_members.first() {
            for (_, member) in &active_members {
                control_member.assert_state(&member);
                println!("{} state equals {} state", control_member.id(), member.id());
            }
        }
    }
});
