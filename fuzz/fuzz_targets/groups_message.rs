// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Display;

use libfuzzer_sys::fuzz_target;
use p2panda_encryption::message_scheme::{GroupEvent, MessageGroup};
use p2panda_encryption::test_utils::message_scheme::dgm::AckedTestDgm;
use p2panda_encryption::test_utils::message_scheme::network::{
    TestGroupError, TestGroupState, init_group_state,
};
use p2panda_encryption::test_utils::message_scheme::ordering::{ForwardSecureOrderer, TestMessage};
use p2panda_encryption::test_utils::{MemberId, MessageId};
use p2panda_encryption::traits::ForwardSecureGroupMessage;
use p2panda_encryption::{Rng, message_scheme};

/// Number of max. group epochs per fuzzing round. Members can create one group operation each per
/// epoch.
const MAX_GROUP_EPOCHS: usize = 128;

/// Max. number of members in a group.
const MAX_GROUP_SIZE: usize = 32;

fn random_u8(rng: &Rng) -> u8 {
    let value: [u8; 1] = rng.random_array().unwrap();
    value[0]
}

fn random_message(rng: &Rng) -> Vec<u8> {
    let length = random_range(3, 32, rng);
    rng.random_vec(length as usize).unwrap()
}

fn random_range(min: u8, max: u8, rng: &Rng) -> u8 {
    let value = random_u8(rng);
    min + (value % (max - min + 1))
}

fn random_item<T: Clone>(vec: Vec<T>, rng: &Rng) -> Option<T> {
    if vec.is_empty() {
        None
    } else {
        let random_index = random_range(0, vec.len() as u8 - 1, rng) as usize;
        Some(vec.get(random_index).cloned().unwrap())
    }
}

fn print_members(members: &[MemberId]) -> String {
    members
        .iter()
        .map(|member| member.to_string())
        .collect::<Vec<String>>()
        .join(", ")
}

#[derive(Debug)]
struct AssertableValues {
    my_id: MemberId,
    members: Vec<MemberId>,
    active_members: Vec<MemberId>,
    removed_members: Vec<MemberId>,
}

impl AssertableValues {
    fn random_member(&self, rng: &Rng) -> Option<MemberId> {
        let members: Vec<MemberId> = self
            .members
            .iter()
            .cloned()
            .filter(|member| {
                !self.active_members.contains(member) && !self.removed_members.contains(member)
            })
            .collect();
        random_item(members, rng)
    }

    fn random_active_member(&self, rng: &Rng) -> Option<MemberId> {
        random_item(self.active_members.clone(), rng)
    }

    fn apply(&mut self, operation: &Operation) {
        match operation {
            Operation::Update | Operation::SendMessage { .. } | Operation::Noop => {
                // Do nothing!
            }
            Operation::Add {
                added,
                members_in_welcome: initial_members,
            } => {
                if added == &self.my_id {
                    // Process "welcome".
                    for member in initial_members {
                        if !self.active_members.contains(member) {
                            self.active_members.push(*member);
                        }
                    }
                }

                if !self.active_members.contains(added) {
                    self.active_members.push(*added);
                }
            }
            Operation::Remove { removed } => {
                if !self.removed_members.contains(removed) {
                    self.removed_members.push(*removed);
                }

                self.active_members = self
                    .active_members
                    .iter()
                    .filter(|member| *member != removed)
                    .cloned()
                    .collect();
            }
            Operation::Create { .. } => unreachable!(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Options {
    Add,
    Remove,
    Update,
    SendMessage,
    Noop,
}

#[derive(Debug)]
struct StateMachine {
    values: AssertableValues,
    history: Vec<Operation>,
    state: State,
}

impl StateMachine {
    pub fn from_standby(my_id: MemberId, members: Vec<MemberId>) -> Self {
        Self {
            values: AssertableValues {
                my_id,
                members,
                active_members: Vec::new(),
                removed_members: Vec::new(),
            },
            history: Vec::new(),
            state: State::Standby,
        }
    }

    pub fn from_create(
        my_id: MemberId,
        members: Vec<MemberId>,
        initial_members: Vec<MemberId>,
    ) -> Self {
        assert!(!members.is_empty());
        assert!(!initial_members.is_empty());

        for member in &initial_members {
            assert!(members.contains(member));
        }

        Self {
            values: AssertableValues {
                my_id,
                members,
                active_members: initial_members.clone(),
                removed_members: Vec::new(),
            },
            history: vec![Operation::Create { initial_members }],
            state: State::Active,
        }
    }

    pub fn is_removed(&self) -> bool {
        matches!(self.state, State::Removed)
    }

    /// Suggest the next group membership operation (adding a member, sending a message, etc.)
    /// based on the current member's state.
    pub fn suggest(&self, rng: &Rng) -> Suggestion {
        let operation = match self.state {
            State::Standby | State::Removed | State::Invalid => Operation::Noop,
            State::Active => self.suggest_valid(
                &[
                    Options::Add,
                    Options::Remove,
                    Options::Update,
                    Options::SendMessage,
                    Options::Noop,
                ],
                rng,
            ),
        };
        Suggestion::Valid(operation)
    }

    /// Randomly suggest a valid, next group operation based on a set of given options.
    fn suggest_valid(&self, try_options: &[Options], rng: &Rng) -> Operation {
        let mut options = Vec::new();

        if try_options.contains(&Options::Add) {
            if let Some(added) = self.values.random_member(rng) {
                options.push(Operation::Add {
                    added,
                    members_in_welcome: self.values.active_members.clone(),
                });
            }
        }

        if try_options.contains(&Options::Remove) {
            if let Some(removed) = self.values.random_active_member(rng) {
                options.push(Operation::Remove { removed });
            }
        }

        if try_options.contains(&Options::Update) {
            options.push(Operation::Update);
        }

        if try_options.contains(&Options::SendMessage) {
            options.push(Operation::SendMessage {
                plaintext: random_message(rng),
            });
        }

        if try_options.contains(&Options::Noop) {
            options.push(Operation::Noop);
        }

        match random_item(options, rng) {
            Some(operation) => operation,
            None => Operation::Noop,
        }
    }

    /// Apply a group operation to the state machine, causing it to transition to a new state and
    /// adjust the expected group state values.
    fn transition(&mut self, operation: &Operation) {
        let next_state = match (&self.state, operation) {
            (State::Standby, Operation::Add { added, .. }) => {
                if added == &self.values.my_id {
                    State::Active
                } else {
                    State::Standby
                }
            }
            (State::Standby, _) => State::Standby,
            (State::Active, Operation::Add { .. }) => State::Active,
            (State::Active, Operation::Remove { removed }) => {
                if removed == &self.values.my_id {
                    State::Removed
                } else {
                    State::Active
                }
            }
            (
                State::Active,
                Operation::Update | Operation::SendMessage { .. } | Operation::Noop,
            ) => State::Active,
            (State::Removed, Operation::Noop) => State::Removed,
            (State::Removed, Operation::Remove { .. }) => State::Removed,
            (_, Operation::Create { .. }) => {
                unreachable!("create can not be called as a transition");
            }
            _ => State::Invalid,
        };

        if matches!(next_state, State::Invalid) {
            panic!("{}: Reached invalid state!", self.values.my_id);
        }

        self.values.apply(operation);

        self.history.push(operation.clone());
        self.state = next_state;
    }

    /// Apply state changes to our machine based on the results of processing a remote message.
    fn transition_remote(
        &mut self,
        added_members: &HashSet<MemberId>,
        removed_members: &HashSet<MemberId>,
    ) {
        for added in added_members {
            self.transition(&Operation::Add {
                added: *added,
                members_in_welcome: vec![],
            });
        }

        for removed in removed_members {
            self.transition(&Operation::Remove { removed: *removed });
        }
    }
}

#[derive(Debug, Clone)]
enum State {
    /// Member was not welcomed to a group yet (either via a "create" or "add" control message).
    Standby,

    /// Member is part of a group and active. They can add and remove other members, update the
    /// group or send messages.
    Active,

    /// Member was removed from a group or removed themselves.
    Removed,

    /// Invalid state.
    Invalid,
}

impl Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                State::Standby => "standby",
                State::Active => "active",
                State::Removed => "removed",
                State::Invalid => "invalid",
            }
        )
    }
}

#[derive(Clone, Debug)]
enum Suggestion {
    Valid(Operation),

    #[allow(dead_code)]
    Invalid(Operation),
}

impl<'a> Suggestion {
    fn operation(&'a self) -> &'a Operation {
        match self {
            Suggestion::Valid(operation) => operation,
            Suggestion::Invalid(operation) => operation,
        }
    }
}

#[derive(Clone, Debug)]
enum Operation {
    Noop,
    Create {
        initial_members: Vec<MemberId>,
    },
    Add {
        added: MemberId,
        members_in_welcome: Vec<MemberId>,
    },
    Remove {
        removed: MemberId,
    },
    Update,
    SendMessage {
        plaintext: Vec<u8>,
    },
}

impl Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Operation::Noop => "noop".to_string(),
                Operation::Create { initial_members } => format!(
                    "create (initial_members={{{}}})",
                    print_members(initial_members)
                ),
                Operation::Add {
                    added,
                    members_in_welcome,
                } => {
                    format!(
                        "add {} (members_in_welcome={{{}}})",
                        added,
                        print_members(members_in_welcome)
                    )
                }
                Operation::Remove { removed } => {
                    format!("remove {}", removed)
                }
                Operation::Update => "update".to_string(),
                Operation::SendMessage { plaintext } => {
                    format!("send message (len={})", plaintext.len())
                }
            }
        )
    }
}

type Message = TestMessage<AckedTestDgm<MemberId, MessageId>>;

type GroupOutput = message_scheme::GroupOutput<
    MemberId,
    MessageId,
    AckedTestDgm<MemberId, MessageId>,
    ForwardSecureOrderer<AckedTestDgm<MemberId, MessageId>>,
>;

#[derive(Debug)]
struct Member {
    machine: StateMachine,
    group: Option<TestGroupState>,
}

impl Member {
    pub fn id(&self) -> MemberId {
        self.machine.values.my_id
    }

    /// Apply and process a local group membership operation for this member.
    ///
    /// This might yield a message which then needs to be broadcast to the group.
    pub fn process_local(
        &mut self,
        operation: &Operation,
        rng: &Rng,
    ) -> Result<Option<Message>, TestGroupError> {
        let y_group = self.group.take().expect("group state exists");

        // Apply and process the local operation.
        let result = match operation {
            Operation::Noop => {
                // Do nothing
                Ok((y_group, None))
            }
            _ => {
                let inner = match operation {
                    Operation::Add { added, .. } => MessageGroup::add(y_group, *added, rng),
                    Operation::Remove { removed } => MessageGroup::remove(y_group, *removed, rng),
                    Operation::Update => MessageGroup::update(y_group, rng),
                    Operation::SendMessage { plaintext } => MessageGroup::send(y_group, plaintext),
                    _ => unreachable!(),
                };
                inner.map(|(y, message)| (y, Some(message)))
            }
        };

        match result {
            Ok((y_group_i, message)) => {
                self.machine.transition(operation);
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
        message: &Message,
        rng: &Rng,
    ) -> Result<Option<GroupOutput>, TestGroupError> {
        if self.machine.is_removed() {
            return Ok(None);
        }

        // Process remote message.
        let y_group = self.group.take().expect("group state exists");
        let (y_group_i, output) = MessageGroup::receive(y_group, message, rng)?;
        self.group.replace(y_group_i);

        if let Some(ref output) = output {
            self.machine
                .transition_remote(&output.added_members, &output.removed_members);
        }

        Ok(output)
    }

    pub fn assert_state(&self, operations: &[(Operation, Message)], outputs: &[GroupOutput]) {
        let y_group = self.group.as_ref().expect("group state exists");

        // Define membership status.
        let members = MessageGroup::members(y_group).expect("members function does not fail");
        let status = {
            let added_in_this_epoch = operations.iter().any(|(operation, _)| {
                if let Operation::Add { added, .. } = operation {
                    added == &self.id()
                } else {
                    false
                }
            });
            let removed_in_this_epoch = operations.iter().any(|(operation, _)| {
                if let Operation::Remove { removed } = operation {
                    removed == &self.id()
                } else {
                    false
                }
            });
            let is_active = members.contains(&self.id());
            MembershipStatus::new(added_in_this_epoch, removed_in_this_epoch, is_active)
        };

        // Expected "members" state.
        let expected_members: HashSet<MemberId> =
            self.machine.values.active_members.iter().cloned().collect();
        assert_eq!(members, expected_members, "member set of {}", self.id());

        // Expected outcomes from operations.
        for (operation, message) in operations {
            if status == MembershipStatus::Inactive {
                continue;
            }

            if let Operation::SendMessage { plaintext } = operation {
                assert_send_message(self.id(), message, plaintext, outputs, status);
            }
        }
    }
}

/// Assert expected outcomes when processing an encrypted message.
fn assert_send_message(
    recipient: MemberId,
    message: &Message,
    expected_plaintext: &[u8],
    outputs: &[GroupOutput],
    status: MembershipStatus,
) {
    // We don't expect decrypted messages in the outputs if we encrypted them ourselves.
    if message.sender() == recipient {
        return;
    }

    // Find a matching decrypted application message in the outputs for this round.
    let application_event = {
        let mut result = None;
        for output in outputs {
            for event in &output.events {
                if let GroupEvent::Application { message_id, .. } = event {
                    if message_id != &message.id() {
                        continue;
                    }
                    result = Some(event.clone());
                }
            }
        }
        result
    };

    match application_event {
        Some(GroupEvent::Application { plaintext, .. }) => {
            // Member should not have received that event as they are either not active or just
            // have been added in this epoch (concurrent messages can not be decrypted).
            if status == MembershipStatus::AddedInEpoch || status == MembershipStatus::Inactive {
                panic!(
                    "unexpected \"application\" output event (recipient_id={}, sender_id={})",
                    recipient,
                    message.sender(),
                );
            }

            // Member was part of the group in the moment the message was sent and thus should
            // be able to decrypt it's contents.
            if status == MembershipStatus::Active || status == MembershipStatus::RemovedInEpoch {
                assert_eq!(
                    &plaintext, expected_plaintext,
                    "unexpected plaintext after decrypting"
                );
            }
        }
        None => {
            // Member was part of the group in the moment the message was sent and should have
            // received an decrypted application event in the outputs.
            //
            // Members who were concurrently removed to the message _can_ still decrypt the
            // content. If this happened depends on if they processed the application message
            // before the removal, this is why we're not checking that here.
            if status == MembershipStatus::Active {
                panic!(
                    "expected \"application\" output event after processing \"send message\" operation (recipient_id={}, sender_id={})",
                    recipient,
                    message.sender(),
                );
            }
        }
        _ => unreachable!(),
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum MembershipStatus {
    /// Member was just added in this epoch. They will be active from next epoch on.
    AddedInEpoch,

    /// Member was just removed in this epoch. They will be inactive from next epoch on.
    RemovedInEpoch,

    /// Member was already active, no change in status.
    Active,

    /// Member is not active, either because they were never welcomed or removed some time ago.
    Inactive,
}

impl MembershipStatus {
    pub fn new(added_in_this_epoch: bool, removed_in_this_epoch: bool, is_active: bool) -> Self {
        if added_in_this_epoch {
            return MembershipStatus::AddedInEpoch;
        }

        if removed_in_this_epoch {
            return MembershipStatus::RemovedInEpoch;
        }

        if !added_in_this_epoch && is_active {
            return MembershipStatus::Active;
        }

        if !removed_in_this_epoch && !is_active {
            return MembershipStatus::Inactive;
        }

        unreachable!();
    }
}

fuzz_target!(|seed: [u8; 32]| {
    let rng = Rng::from_seed(seed);

    // Generate a list of all members.
    let mut members: HashMap<MemberId, Member> = HashMap::new();
    let member_ids = {
        let mut buf = Vec::with_capacity(MAX_GROUP_SIZE);
        for i in 0..MAX_GROUP_SIZE {
            buf.push(i);
        }
        buf
    };

    // Pick a random group creator.
    let group_creator = random_item(member_ids.clone(), &rng).unwrap();

    // Initialise group encryption states.
    let group_states = {
        let members: [MemberId; MAX_GROUP_SIZE] = member_ids.clone().try_into().unwrap();
        init_group_state::<MAX_GROUP_SIZE>(members, &rng)
    };

    let mut queue = VecDeque::new();

    for id in &member_ids {
        members.insert(
            *id,
            Member {
                // Initialise state machine for each member.
                machine: if id == &group_creator {
                    StateMachine::from_create(*id, member_ids.clone(), vec![*id])
                } else {
                    StateMachine::from_standby(*id, member_ids.clone())
                },
                // Set up group state for each member.
                group: {
                    if id == &group_creator {
                        // The group "creator" initialises the group with themselves ..
                        let (y_group_i, message) =
                            MessageGroup::create(group_states[*id].clone(), vec![*id], &rng)
                                .unwrap();

                        // .. and publishes the first "create" control message on the test network.
                        queue.push_back((
                            Suggestion::Valid(Operation::Create {
                                initial_members: vec![*id],
                            }),
                            message,
                        ));

                        Some(y_group_i)
                    } else {
                        Some(group_states[*id].clone())
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

    for _ in 0..MAX_GROUP_EPOCHS {
        // 1. Go through all members of the group, suggest and apply a local operation this member
        //    can do. Inactive or removed members will not cause any actions.

        let mut concurrent_adds = HashSet::new();
        let mut operations: Vec<(Operation, Message)> = Vec::new();

        for member_id in &member_ids {
            let member = members.get_mut(member_id).expect("member exists");

            // Suggest the next group membership operation for this member.
            let suggestion = member.machine.suggest(&rng);

            // TODO(adz): Disallow concurrent adds of the same member for now, our (terrible) DGM
            // implementation in test_utils is not handling that well.
            if let Suggestion::Valid(Operation::Add { added, .. }) = suggestion {
                if concurrent_adds.contains(&added) {
                    continue;
                }
                concurrent_adds.insert(added);
            }

            if !matches!(suggestion.operation(), Operation::Noop) {
                println!(
                    "member: {}, operation: {}",
                    member.machine.values.my_id,
                    suggestion.operation(),
                );
            }

            // Process group operation locally for this member.
            match &suggestion {
                Suggestion::Valid(operation) => {
                    if let Some(message) = member
                        .process_local(operation, &rng)
                        .unwrap_or_else(|_| panic!("valid operations to not fail: {}", operation))
                    {
                        operations.push((operation.clone(), message.clone()));
                        queue.push_back((suggestion, message));
                    }
                }
                Suggestion::Invalid(operation) => {
                    assert!(
                        member.process_local(operation, &rng).is_err(),
                        "expected error due to invalid group operation"
                    );
                }
            }
        }

        // 2. Processing all local operations might have created a couple of messages which now
        //    need to be "broadcast" to all members, which will process each of them as well.
        //
        //    By processing remote operations members might yield new messages for the group. We
        //    loop over the message queue until all messages have been processed.
        //
        //    With this setup we will _always_ process all group operations after one round.
        //    Concurrent operations can then only happen within this round. This is a simplified
        //    fuzzing setup not simulating more complex concurrent p2p scenarios.

        let mut outputs: HashMap<MemberId, Vec<GroupOutput>> = HashMap::new();

        while let Some((suggestion, message)) = queue.pop_front() {
            // println!(
            //     "next message from queue: \"{}\" sent by {}",
            //     message.encryption_content(),
            //     message.sender()
            // );

            for member_id in &member_ids {
                // Do not process our own messages.
                if member_id == &message.sender() {
                    continue;
                }

                let member = members.get_mut(member_id).expect("member exists");
                match member.process_remote(&message, &rng) {
                    Ok(output) => {
                        if let Suggestion::Invalid(operation) = suggestion {
                            panic!(
                                "expected error when processing remote message from invalid operation '{}'",
                                operation
                            )
                        }

                        if let Some(output) = output {
                            // There might be more messages to-be-broadcast after processing. Let's
                            // queue them up!
                            for event in &output.events {
                                if let message_scheme::GroupEvent::Control(output_message) = event {
                                    queue.push_back((suggestion.clone(), output_message.clone()));
                                }
                            }

                            // Keep all outputs per member for later to assert.
                            outputs
                                .entry(*member_id)
                                .and_modify(|outputs| outputs.push(output.clone()))
                                .or_insert(vec![output]);
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
                    }
                }
            }
        }

        // 3. Assert outcomes.

        for member_id in &member_ids {
            let member = members.get_mut(member_id).expect("member exists");
            member.assert_state(&operations, outputs.get(member_id).unwrap_or(&vec![]));
        }

        if !outputs.is_empty() {
            println!("--------");
        }
    }
});
