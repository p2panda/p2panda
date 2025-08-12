// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Display;
use std::fs::File;
use std::io::Write;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use libfuzzer_sys::fuzz_target;
use p2panda_auth::group::{
    GroupAction, GroupControlMessage, GroupCrdtError, GroupCrdtState, GroupMember,
};
use p2panda_auth::test_utils::partial_ord::{
    TestGroup, TestGroupError, TestGroupState, TestOrderer,
};
use p2panda_auth::test_utils::{MemberId, MessageId, TestOperation};
use p2panda_auth::traits::Operation as OperationTrait;
use p2panda_auth::{Access, AccessLevel};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng, random_bool};

/// Flag for saving dot graph representations of all groups to the filesystem.
///
/// Graphs are saved when an error occurs in any case.
const SAVE_GRAPH_VIZ: bool = true;

/// Pool of all possible group member ids.
const MEMBERS: [char; 26] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
];

/// The root group id.
const ROOT_GROUP_ID: char = '0';

/// Max number of "rounds" in which members can publish operations.
const MAX_ACTION_ROUNDS: usize = 6;

/// Max operations per actor, per round.
const MAX_ACTOR_OPERATIONS_PER_ROUND: u8 = 2;

/// Max concurrent branches.
const MAX_BRANCHES: u8 = 6;

/// Possible access levels.
const ACCESS_LEVELS: [Access<()>; 4] = [
    Access {
        conditions: None,
        level: AccessLevel::Pull,
    },
    Access {
        conditions: None,
        level: AccessLevel::Read,
    },
    Access {
        conditions: None,
        level: AccessLevel::Write,
    },
    Access {
        conditions: None,
        level: AccessLevel::Manage,
    },
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

fn random_member_type(id: MemberId) -> GroupMember<MemberId> {
    if random_bool(1.0 / 3.0) {
        GroupMember::Group(id)
    } else {
        GroupMember::Individual(id)
    }
}

fn print_members(members: &[(GroupMember<MemberId>, Access<()>)]) -> String {
    members
        .iter()
        .map(|(id, access)| format!("{id:?} {access}"))
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

// @TODO: we can probably remove this.
#[derive(Clone, Debug)]
enum Suggestion {
    Valid(TestGroupAction),

    #[allow(dead_code)]
    Invalid(TestGroupAction),
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
                            format!("add {member:?}",)
                        }
                        GroupAction::Remove { member } => {
                            format!("remove {member:?}")
                        }
                        GroupAction::Promote { member, .. } => format!("promote {member:?}"),
                        GroupAction::Demote { member, .. } => format!("demote {member:?}"),
                    }
                }
            }
        )
    }
}

/// A group member.
///
/// Group members have their own independent state, and can be added to a group as an individual
/// or group.
#[derive(Clone, Debug)]
struct Member {
    /// Member id.
    my_id: MemberId,

    /// All possible group members.
    members: Vec<GroupMember<MemberId>>,

    /// Group store.
    auth_y: TestGroupState,

    auth_heads_ref: Rc<RefCell<Vec<MessageId>>>,

    /// IDs of all operations processed by this member.
    processed: Vec<MessageId>,
}

impl Member {
    /// Instantiate a new member.
    pub fn new(
        my_id: MemberId,
        members: Vec<GroupMember<MemberId>>,
        creator_id: MemberId, // creator of the root group.
        operations: &mut HashMap<MessageId, (Suggestion, TestOperation)>,
        rng: &mut StdRng,
    ) -> Self {
        let auth_heads_ref = Rc::new(RefCell::new(vec![]));
        let orderer_y = TestOrderer::init(my_id, auth_heads_ref.clone(), StdRng::from_rng(rng));
        let auth_y = GroupCrdtState::new(orderer_y);

        let mut member = Member {
            my_id,
            members: members.clone(),
            auth_y,
            auth_heads_ref,
            processed: Vec::new(),
        };

        // If we are the creator then instantiate the root group and generate the create operations.
        if my_id == creator_id {
            // Calculate number of initial group members.
            let group_members_count = random_range(0, members.len() as u8, rng) / 2;

            // Generate initial members with member type and access levels.
            let mut initial_members = members.clone();
            let _ = initial_members.split_off(group_members_count as usize);
            initial_members.retain(|member| {
                // Don't include members which are groups as we won't have there group state yet.
                if let GroupMember::Group(_) = member {
                    return false;
                };

                // Ignore if we were added already in order to add ourselves in the next step as
                // an admin member.
                member.id() != my_id
            });
            let mut initial_members = initial_members
                .into_iter()
                .map(|member| (member, random_item(ACCESS_LEVELS.to_vec(), rng).unwrap()))
                .collect::<Vec<_>>();

            // Add ourselves as admin member.
            initial_members.push((GroupMember::Individual(member.my_id), Access::manage()));

            member.create_group(ROOT_GROUP_ID, initial_members, operations);
        }

        if member.is_group() {
            // Group create a sub-group for themselves incase they should be added as a
            // sub-group.
            member.create_group(
                member.id(),
                vec![(GroupMember::Individual(member.my_id), Access::manage())],
                operations,
            );
        }

        member
    }

    /// Create a group containing passed initial members.
    pub fn create_group(
        &mut self,
        group_id: MemberId,
        initial_members: Vec<(GroupMember<MemberId>, Access<()>)>,
        operations: &mut HashMap<MessageId, (Suggestion, TestOperation)>,
    ) {
        let control_message = GroupControlMessage {
            group_id,
            action: GroupAction::Create {
                initial_members: initial_members.clone(),
            },
        };

        let (auth_y_i, operation) =
            TestGroup::prepare(self.auth_y.clone(), &control_message).unwrap();
        let auth_y_ii = TestGroup::process(auth_y_i, &operation).unwrap();

        self.auth_y = auth_y_ii;
        self.auth_heads_ref
            .replace(self.auth_y.auth_y.heads().into_iter().collect());

        let suggestion = Suggestion::Valid(TestGroupAction::Action(GroupAction::Create {
            initial_members,
        }));

        self.processed.push(operation.id());
        operations.insert(operation.id(), (suggestion, operation));
    }

    /// Id of this member.
    pub fn id(&self) -> MemberId {
        self.my_id
    }

    pub fn is_group(&self) -> bool {
        let member = self.members.iter().find(|m| m.id() == self.id()).unwrap();

        match member {
            GroupMember::Individual(_) => false,
            GroupMember::Group(_) => true,
        }
    }

    /// Get the members of a group.
    pub fn members(&self, group_id: MemberId) -> Vec<(MemberId, Access<()>)> {
        self.auth_y.members(group_id)
    }

    pub fn root_members(&self, group_id: MemberId) -> Vec<(GroupMember<MemberId>, Access<()>)> {
        self.auth_y.root_members(group_id)
    }

    /// Is this member in a group.
    pub fn is_member(&self, group_id: MemberId) -> bool {
        self.members(group_id)
            .iter()
            .any(|(id, _)| id == &self.id())
    }

    /// Is this member a manager in a group.
    pub fn is_manager(&self, group_id: MemberId) -> bool {
        self.members(group_id)
            .iter()
            .any(|(id, access)| id == &self.id() && access == &Access::manage())
    }

    /// Process an operation created locally.
    pub fn process_local(
        &mut self,
        group_id: MemberId,
        operation: &TestGroupAction,
    ) -> Result<Option<TestOperation>, TestGroupError> {
        let result = match operation {
            TestGroupAction::Noop => Ok(None),
            TestGroupAction::Action(action) => {
                let control_message = GroupControlMessage {
                    group_id,
                    action: action.clone(),
                };

                let (auth_y_i, operation) =
                    TestGroup::prepare(self.auth_y.clone(), &control_message).unwrap();
                let auth_y_ii = match TestGroup::process(auth_y_i, &operation) {
                    Ok(y) => y,
                    Err(err) => {
                        self.report(group_id, true);
                        println!("{:#?}", operation);
                        panic!("{err}");
                    }
                };
                self.auth_y = auth_y_ii;
                self.auth_heads_ref
                    .replace(self.auth_y.auth_y.heads().into_iter().collect());

                Ok(Some(operation))
            }
        };

        match result {
            Ok(operation) => {
                if let Some(operation) = operation.as_ref() {
                    self.processed.push(operation.id());
                }

                Ok(operation)
            }
            Err(err) => Err(err),
        }
    }

    /// Process an operation created by a different actor.
    pub fn process_remote(
        &mut self,
        suggestion: &Suggestion,
        operation: &TestOperation,
    ) -> Result<(), TestGroupError> {
        if self.processed.iter().any(|id| *id == operation.id()) {
            return Ok(());
        }

        self.auth_y = match TestGroup::process(self.auth_y.clone(), operation) {
            Ok(y) => {
                if let Suggestion::Invalid(_) = suggestion {
                    panic!(
                        "expected error when processing remote operation from invalid operation '{operation:?}'"
                    )
                };

                y
            }
            Err(err) => {
                if let GroupCrdtError::DuplicateOperation(_, _) = err {
                    self.auth_y.clone()
                } else {
                    if let Suggestion::Valid(_) = suggestion {
                        self.report(ROOT_GROUP_ID, true);

                        panic!(
                            "unexpected error when processing remote operation from valid operation member={} '{:?}':\n{}",
                            self.id(),
                            operation,
                            err
                        );
                    }
                    self.auth_y.clone()
                }
            }
        };
        self.auth_heads_ref
            .replace(self.auth_y.auth_y.heads().into_iter().collect());

        self.processed.push(operation.id());

        Ok(())
    }

    /// Assert our root group state is the same as another member.
    pub fn assert_state(&self, other: &Member) {
        let mut other_members = other.members(ROOT_GROUP_ID);
        other_members.sort();

        let mut members = self.members(ROOT_GROUP_ID);
        members.sort();

        if members != other_members {
            println!("member set of {} compared to {} ", self.id(), other.id());
            println!();
            self.report(ROOT_GROUP_ID, true);
            other.report(ROOT_GROUP_ID, true);
        }

        assert_eq!(members, other_members,);
    }

    /// Get a random member of the passed group.
    fn random_member(
        &self,
        group_id: MemberId,
        rng: &mut StdRng,
    ) -> Option<(GroupMember<MemberId>, Access<()>)> {
        random_item(self.root_members(group_id), rng)
    }

    /// Get a random non-member of the passed group.
    fn random_non_member(
        &self,
        group_id: MemberId,
        rng: &mut StdRng,
    ) -> Option<GroupMember<MemberId>> {
        let active_members = self.root_members(group_id);
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

    /// Suggest the next group membership operation for the passed group based on the current
    /// member's state and this members'  current access level (only members with manage access
    /// can perform group actions).
    pub fn suggest(&self, group_id: MemberId, rng: &mut StdRng) -> Suggestion {
        let operation = if self.is_manager(group_id) {
            self.suggest_valid(
                group_id,
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

    /// Randomly suggest a valid, next group operation based on a set of given options.
    fn suggest_valid(
        &self,
        group_id: MemberId,
        try_options: &[Options],
        rng: &mut StdRng,
    ) -> TestGroupAction {
        let mut options = Vec::new();

        let Some((_, access)) = self
            .members(group_id)
            .into_iter()
            .find(|(member, _)| *member == self.my_id)
        else {
            return TestGroupAction::Noop;
        };

        if access < Access::manage() {
            return TestGroupAction::Noop;
        }

        if try_options.contains(&Options::Add) {
            if let Some(member) = self.random_non_member(group_id, rng) {
                let access = match member {
                    GroupMember::Individual(_) => {
                        random_item(ACCESS_LEVELS.to_vec(), rng).unwrap()
                    }
                    GroupMember::Group(_) => {
                        random_item(vec![Access::pull(), Access::read(), Access::write()], rng)
                            .unwrap()
                    }
                };

                if member.id() != self.my_id {
                    options.push(TestGroupAction::Action(GroupAction::Add { member, access }))
                }
            }
        }

        if try_options.contains(&Options::Promote) {
            if let Some((member, access)) = self.random_member(group_id, rng) {
                loop {
                    if access.is_manage() {
                        break;
                    }

                    let next_access = match member {
                        GroupMember::Individual(_) => {
                            random_item(ACCESS_LEVELS.to_vec(), rng).unwrap()
                        }
                        GroupMember::Group(_) => {
                            random_item(vec![Access::pull(), Access::read(), Access::write()], rng)
                                .unwrap()
                        }
                    };

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
            if let Some((member, access)) = self.random_member(group_id, rng) {
                loop {
                    if access.is_pull() {
                        break;
                    }

                    let next_access = match member {
                        GroupMember::Individual(_) => {
                            random_item(ACCESS_LEVELS.to_vec(), rng).unwrap()
                        }
                        GroupMember::Group(_) => {
                            random_item(vec![Access::pull(), Access::read(), Access::write()], rng)
                                .unwrap()
                        }
                    };

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
            if let Some(removed) = self.random_member(group_id, rng) {
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

    /// Print a report for this member.
    fn report(&self, group_id: MemberId, save_graph: bool) {
        println!("=== {} final members for group {} ===", self.id(), group_id);
        println!("{:?}", self.members(group_id));
        println!();
        println!("=== filter ===");
        let mut filter = self.auth_y.auth_y.ignore.iter().collect::<Vec<_>>();
        filter.sort();
        println!("{filter:?}");
        println!();

        if save_graph {
            let mut file = File::create(format!(
                "{}_group_{}_{}.txt",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis(),
                group_id,
                self.id()
            ))
            .unwrap();
            file.write_all(self.auth_y.display(group_id).as_bytes())
                .unwrap();
        }
    }
}

/// Sync a set of members.
///
/// This involves all members processing any operations other members have processed but they have
/// not. Operations are processed in the order they were created/processed per peer.
fn sync(
    members_to_sync: &Vec<MemberId>,
    members: &mut HashMap<MemberId, Member>,
    operations: &HashMap<MessageId, (Suggestion, TestOperation)>,
) {
    for partition_member in members_to_sync {
        for other_partition_member in members_to_sync {
            if partition_member == other_partition_member {
                continue;
            }

            let other_processed = members
                .get(other_partition_member)
                .expect("member exists")
                .processed
                .clone();

            for id in other_processed {
                let (suggestion, operation) = operations.get(&id).unwrap();
                let member = members.get_mut(partition_member).expect("member exists");
                if member.id() == operation.author() {
                    continue;
                }

                member.process_remote(suggestion, operation).unwrap();
            }
        }
    }
}

fuzz_target!(|seed: [u8; 32]| {
    let mut rng = StdRng::from_seed(seed);

    // Generate a list of all member ids.
    let mut members: HashMap<MemberId, Member> = HashMap::new();
    let range: u8 = random_range(1, MEMBERS.len() as u8, &mut rng);
    let mut member_ids = MEMBERS[0..range as usize].to_vec();

    // Pop off the root group creator.
    let group_creator = member_ids.pop().unwrap();

    // Assign all members as either "individual" or "group". This signifies how they would be
    // added to a group.
    let mut member_ids: Vec<GroupMember<MemberId>> =
        member_ids.into_iter().map(random_member_type).collect();

    // Push back the root group creator as an individual.
    member_ids.push(GroupMember::Individual(group_creator));

    // Map containing all operations.
    let mut operations = HashMap::new();

    // Instantiate all members.
    for member in &member_ids {
        members.insert(
            member.id(),
            Member::new(
                member.id(),
                member_ids.clone(),
                group_creator,
                &mut operations,
                &mut rng,
            ),
        );
    }

    // Sync all members so that they all get the initial root group and each others' initial
    // sub-group states.
    sync(
        &member_ids
            .clone()
            .iter()
            .map(|member| member.id())
            .collect(),
        &mut members,
        &operations,
    );

    for _ in 0..MAX_ACTION_ROUNDS {
        // Calculate next partitions.
        //
        // Partitions are how members are grouped, only members in the same partition sync
        // messages (per round).
        let mut partition_map: HashMap<u8, Vec<MemberId>> = HashMap::new();
        for member_id in &member_ids {
            let partition_id = random_range(1, MAX_BRANCHES, &mut rng);
            partition_map
                .entry(partition_id)
                .or_default()
                .push(member_id.id());
        }

        // Process all operations pushed to our current partition.
        for partition_members in partition_map.values() {
            sync(partition_members, &mut members, &operations);
        }

        // Each member suggests a next operations and pushes them to the global partition queue.
        for partition_members in partition_map.values() {
            for _ in 0..random_range(1, MAX_ACTOR_OPERATIONS_PER_ROUND, &mut rng) {
                for partition_member in partition_members {
                    let (suggestion, group_id) = {
                        let member = members.get(partition_member).unwrap();

                        // Check if we are an admin member of a sub-group so that we can
                        // optionally publish an operation to the sub-group (rather than the root
                        // group).
                        let members = member.root_members(ROOT_GROUP_ID);
                        let is_sub_group_admin = members.iter().find_map(|(group_member, _)| {
                            if let GroupMember::Group(id) = group_member {
                                if member.is_manager(*id) {
                                    return Some(id);
                                }
                            };
                            None
                        });

                        let mut group_id = ROOT_GROUP_ID;

                        // Either publish an operation to our own group, a sub-group we're an admin member of, or the root group.
                        if let Some(sub_group) = is_sub_group_admin {
                            group_id =
                                random_item(vec![ROOT_GROUP_ID, *sub_group], &mut rng).unwrap();
                        };

                        let suggestion = member.suggest(group_id, &mut rng);
                        (suggestion, group_id)
                    };

                    // Process group operation locally for this member.
                    match &suggestion {
                        Suggestion::Valid(action) => {
                            let member = members.get_mut(partition_member).unwrap();

                            if let Some(operation) = member
                                .process_local(group_id, action)
                                .unwrap_or_else(|error| {
                                    println!("group={group_id}, action={action:?}");
                                    member.report(group_id, true);
                                    panic!("valid actions to not fail: {error}")
                                })
                            {
                                // All other partition members process it.
                                for other_partition_member in partition_members {
                                    if partition_member == other_partition_member {
                                        continue;
                                    }
                                    let other_member =
                                        members.get_mut(other_partition_member).unwrap();

                                    other_member
                                        .process_remote(&suggestion, &operation)
                                        .unwrap();
                                }

                                operations.insert(operation.id(), (suggestion, operation));
                            }
                        }
                        Suggestion::Invalid(operation) => {
                            let member = members.get_mut(partition_member).unwrap();
                            assert!(
                                member.process_local(ROOT_GROUP_ID, operation).is_err(),
                                "expected error due to invalid group operation"
                            );
                        }
                    }
                }
            }
        }

        // Assert all partition members have equal group state.
        for partition_members in partition_map.values() {
            let active_members: Vec<(&char, &Member)> = members
                .iter()
                .filter(|(_, member)| {
                    member.is_member(ROOT_GROUP_ID) && partition_members.contains(&member.id())
                })
                .collect();

            if let Some((_, control_member)) = active_members.first() {
                for partition_member in partition_members {
                    let partition_member = members.get(partition_member).expect("member exists");
                    control_member.assert_state(partition_member);
                }
            }
        }
    }

    // Sync all members.
    let member_ids: Vec<MemberId> = members.keys().cloned().collect();
    sync(&member_ids, &mut members, &operations);

    let members_count = members.len();
    let mut active_members: Vec<(char, Member)> = members
        .into_iter()
        .filter(|(_, member)| member.is_member(ROOT_GROUP_ID))
        .collect();

    // Assert all group members have the same state.
    if let Some((_, control_member)) = active_members.pop() {
        for (_, member) in &active_members {
            control_member.assert_state(member);
        }

        println!("=== test setup ===");
        println!("group: {:?}", ROOT_GROUP_ID);
        println!("actors: {members_count:?}");
        println!("branches: {MAX_BRANCHES:?}");
        println!(
            "operations: {:?}",
            control_member.processed.len() + control_member.processed.len()
        );
        println!();
        control_member.report(ROOT_GROUP_ID, SAVE_GRAPH_VIZ);
    }

    drop(active_members);
    drop(operations);
});
