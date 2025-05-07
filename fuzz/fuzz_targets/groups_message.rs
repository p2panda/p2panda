// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Display;

use libfuzzer_sys::fuzz_target;
use p2panda_group::message_scheme::MessageGroup;
use p2panda_group::test_utils::message_scheme::dgm::AckedTestDgm;
use p2panda_group::test_utils::message_scheme::network::{
    TestGroupError, TestGroupState, init_group_state,
};
use p2panda_group::test_utils::message_scheme::ordering::{ForwardSecureOrderer, TestMessage};
use p2panda_group::test_utils::{MemberId, MessageId};
use p2panda_group::traits::ForwardSecureGroupMessage;
use p2panda_group::{Rng, message_scheme};

const INVALID_TRANSITION_CHANCE: u8 = 0; // in %

const MAX_OPERATIONS: usize = 3;

const MAX_GROUP_SIZE: usize = 4;

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
struct Values {
    my_id: MemberId,
    members: Vec<MemberId>,
    active_members: Vec<MemberId>,
    removed_members: Vec<MemberId>,
    is_active: bool,
}

impl Values {
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

    fn process_valid(&mut self, operation: &Operation) {
        println!("process_valid {} {}", self.my_id, operation);

        match operation {
            Operation::Update | Operation::SendMessage { .. } | Operation::Noop => {
                // Do nothing!
            }
            Operation::Add {
                added,
                initial_members,
            } => {
                // assert!(self.members.contains(added));
                // assert!(!self.active_members.contains(added));
                // assert!(!self.removed_members.contains(added));
                if added == &self.my_id {
                    // Process "welcome".
                    // assert!(self.active_members.is_empty());
                    // assert!(self.removed_members.is_empty());
                    for member in initial_members {
                        assert!(self.members.contains(member));
                    }
                    self.is_active = true;
                    self.active_members = initial_members.to_vec();
                }

                if !self.active_members.contains(added) {
                    self.active_members.push(*added);
                }
            }
            Operation::Remove { removed } => {
                // assert!(self.members.contains(removed));
                // assert!(self.active_members.contains(removed));
                // assert!(!self.removed_members.contains(removed));
                if !self.removed_members.contains(removed) {
                    self.removed_members.push(*removed);
                }
                self.active_members = self
                    .active_members
                    .iter()
                    .filter(|member| *member != removed)
                    .cloned()
                    .collect();
                if removed == &self.my_id {
                    self.is_active = false;
                }
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
struct Machine {
    values: Values,
    history: Vec<Operation>,
    state: State,
}

impl Machine {
    pub fn from_standby(my_id: MemberId, members: Vec<MemberId>) -> Self {
        Self {
            values: Values {
                my_id,
                members,
                active_members: Vec::new(),
                removed_members: Vec::new(),
                is_active: false,
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
        assert!(members.len() > 0);
        assert!(initial_members.len() > 0);

        for member in &initial_members {
            assert!(members.contains(member));
        }

        Self {
            values: Values {
                my_id,
                members,
                active_members: initial_members.clone(),
                removed_members: Vec::new(),
                is_active: true,
            },
            history: vec![Operation::Create { initial_members }],
            state: State::CreatedGroup,
        }
    }

    pub fn suggest(&self, chance_for_invalid: u8, rng: &Rng) -> Suggestion {
        assert!(chance_for_invalid <= 100);
        let suggest_valid = random_range(1, 100, rng) > chance_for_invalid;

        if suggest_valid {
            let operation = match self.state {
                State::Standby => Operation::Noop,
                State::CreatedGroup
                | State::Welcomed
                | State::AddedMember
                | State::RemovedMember
                | State::SentMessage
                | State::Invalid => self.suggest_valid(
                    &[
                        Options::Add,
                        Options::Remove,
                        Options::Update,
                        Options::SendMessage,
                        Options::Noop,
                    ],
                    rng,
                ),
                State::UpdatedGroup => {
                    self.suggest_valid(&[Options::Add, Options::Remove, Options::SendMessage], rng)
                }
            };
            Suggestion::Valid(operation)
        } else {
            Suggestion::Invalid(self.suggest_invalid(rng))
        }
    }

    fn suggest_valid(&self, try_options: &[Options], rng: &Rng) -> Operation {
        if !self.values.is_active {
            return Operation::Noop;
        }

        let mut options = Vec::new();

        if try_options.contains(&Options::Add) {
            if let Some(added) = self.values.random_member(rng) {
                options.push(Operation::Add {
                    added,
                    initial_members: self.values.active_members.clone(),
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

    fn suggest_invalid(&self, _rng: &Rng) -> Operation {
        // TODO
        Operation::Noop
    }

    fn transition(&mut self, operation: Operation) {
        let next_state = match (&self.state, &operation) {
            (State::Standby, Operation::Add { added, .. }) => {
                if added == &self.values.my_id {
                    State::Welcomed
                } else {
                    State::Invalid
                }
            }
            (
                State::CreatedGroup
                | State::Welcomed
                | State::AddedMember
                | State::RemovedMember
                | State::UpdatedGroup
                | State::SentMessage,
                Operation::Add { .. },
            ) => State::AddedMember,
            (
                State::CreatedGroup
                | State::Welcomed
                | State::AddedMember
                | State::RemovedMember
                | State::UpdatedGroup
                | State::SentMessage,
                Operation::Remove { .. },
            ) => State::RemovedMember,
            (
                State::CreatedGroup
                | State::Welcomed
                | State::AddedMember
                | State::RemovedMember
                | State::SentMessage,
                Operation::Update,
            ) => State::UpdatedGroup,
            (
                State::CreatedGroup
                | State::Welcomed
                | State::AddedMember
                | State::RemovedMember
                | State::UpdatedGroup
                | State::SentMessage,
                Operation::SendMessage { .. },
            ) => State::SentMessage,
            (
                State::CreatedGroup
                | State::Welcomed
                | State::AddedMember
                | State::RemovedMember
                | State::SentMessage,
                Operation::Noop,
            ) => self.state.clone(),
            (_, Operation::Create { .. }) => {
                unreachable!("create can not be called as a transition");
            }
            _ => State::Invalid,
        };

        println!("{}: {} > {}", self.values.my_id, self.state, next_state);
        self.values.process_valid(&operation);

        if self.values.is_active {
            self.history.push(operation);
            self.state = next_state;
        }
    }

    fn process_output(
        &mut self,
        added_members: &HashSet<MemberId>,
        removed_members: &HashSet<MemberId>,
    ) {
        for added in added_members {
            if added == &self.values.my_id {
                continue;
            }

            self.values.process_valid(&Operation::Add {
                added: *added,
                initial_members: vec![],
            });
        }

        for removed in removed_members {
            if removed == &self.values.my_id {
                continue;
            }

            self.values
                .process_valid(&Operation::Remove { removed: *removed });
        }
    }
}

#[derive(Debug, Clone)]
enum State {
    Standby,
    CreatedGroup,
    Welcomed,
    AddedMember,
    RemovedMember,
    UpdatedGroup,
    SentMessage,
    Invalid,
}

impl Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            State::Standby => "standby",
            State::CreatedGroup => "created",
            State::Welcomed => "welcomed",
            State::AddedMember => "added",
            State::RemovedMember => "removed",
            State::UpdatedGroup => "updated",
            State::SentMessage => "sent",
            State::Invalid => "invalid",
        })
    }
}

#[derive(Clone, Debug)]
enum Suggestion {
    Valid(Operation),
    Invalid(Operation),
}

impl Suggestion {
    fn operation(&self) -> Operation {
        match self {
            Suggestion::Valid(operation) => operation.clone(),
            Suggestion::Invalid(operation) => operation.clone(),
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
        initial_members: Vec<MemberId>,
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
        write!(f, "{}", match self {
            Operation::Noop => "noop".to_string(),
            Operation::Create { initial_members } => format!(
                "create (initial_members={{{}}})",
                print_members(initial_members)
            ),
            Operation::Add {
                added,
                initial_members,
            } => {
                format!(
                    "add {} (members_in_welcome={{{}}})",
                    added,
                    print_members(initial_members)
                )
            }
            Operation::Remove { removed } => {
                format!("remove {}", removed)
            }
            Operation::Update => "update".to_string(),
            Operation::SendMessage { plaintext } => {
                format!("send message (len={})", plaintext.len())
            }
        })
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
    machine: Machine,
    group: TestGroupState,
    is_removed: bool,
}

impl Member {
    pub fn id(&self) -> MemberId {
        self.machine.values.my_id
    }

    pub fn next(
        &mut self,
        operation: &Operation,
        rng: &Rng,
    ) -> Result<Option<Message>, TestGroupError> {
        let result = match operation {
            Operation::Add { added, .. } => MessageGroup::add(self.group.clone(), *added, &rng),
            Operation::Remove { removed } => {
                if removed == &self.id() {
                    self.is_removed = true;
                }
                MessageGroup::remove(self.group.clone(), *removed, &rng)
            }
            Operation::Update => MessageGroup::update(self.group.clone(), &rng),
            Operation::SendMessage { plaintext } => {
                MessageGroup::send(self.group.clone(), &plaintext)
            }
            Operation::Noop => {
                // Do nothing
                return Ok(None);
            }
            Operation::Create { .. } => {
                unreachable!()
            }
        };

        match result {
            Ok((group_i, message)) => {
                self.machine.transition(operation.clone());
                self.group = group_i;
                Ok(Some(message))
            }
            Err(err) => Err(err),
        }
    }

    pub fn process_operation(&mut self, operation: &Operation) {
        if self.is_removed {
            return;
        }

        // Tell state machine if we've been added ("welcomed").
        if let Operation::Add { added, .. } = operation {
            if added == &self.id() {
                self.machine.transition(operation.clone());
            }
        }

        // Tell state machine about our own removal.
        if let Operation::Remove { removed } = operation {
            if removed == &self.id() {
                self.machine.transition(operation.clone());
            }
        }
    }

    pub fn process(
        &mut self,
        message: &Message,
        rng: &Rng,
    ) -> Result<Option<GroupOutput>, TestGroupError> {
        if self.is_removed {
            return Ok(None);
        }

        let (group_i, output) = MessageGroup::receive(self.group.clone(), message, rng)?;
        self.group = group_i;

        if let Some(ref output) = output {
            for event in &output.events {
                if let message_scheme::GroupEvent::RemovedOurselves = event {
                    self.is_removed = true;
                }
            }

            self.machine
                .process_output(&output.added_members, &output.removed_members);
        }

        Ok(output)
    }

    pub fn assert_process(&mut self, _operation: &Operation, _output: &Option<GroupOutput>) {
        // match operation {
        //     Operation::Add {
        //         added,
        //         initial_members,
        //     } => todo!(),
        //     Operation::Remove { removed } => todo!(),
        //     Operation::Update => todo!(),
        //     Operation::SendMessage { plaintext } => todo!(),
        //     Operation::Noop | Operation::Create { .. } => (),
        // }
    }
}

fuzz_target!(|seed: [u8; 32]| {
    let rng = Rng::from_seed(seed);

    let mut members: HashMap<MemberId, Member> = HashMap::new();

    let member_ids = {
        let mut buf = Vec::with_capacity(MAX_GROUP_SIZE);
        for i in 0..MAX_GROUP_SIZE {
            buf.push(i);
        }
        buf
    };

    let mut queue = VecDeque::new();

    // Pick a random group creator.
    let group_creator = random_item(member_ids.clone(), &rng).unwrap();

    // Initialise group encryption states.
    let group_states = {
        let members: [MemberId; MAX_GROUP_SIZE] = member_ids.clone().try_into().unwrap();
        init_group_state::<MAX_GROUP_SIZE>(members, &rng)
    };

    // Initialise state machines for each member.
    for id in &member_ids {
        members.insert(*id, Member {
            machine: if id == &group_creator {
                Machine::from_create(*id, member_ids.clone(), vec![*id])
            } else {
                Machine::from_standby(*id, member_ids.clone())
            },
            group: {
                if id == &group_creator {
                    let (group_i, message) =
                        MessageGroup::create(group_states[*id].clone(), vec![*id], &rng).unwrap();

                    queue.push_back((
                        Suggestion::Valid(Operation::Create {
                            initial_members: vec![*id],
                        }),
                        message,
                    ));

                    group_i
                } else {
                    group_states[*id].clone()
                }
            },
            is_removed: false,
        });
    }

    println!("\n==============================");
    println!("group created [group_creator={}]", group_creator);
    println!("==============================");

    for _ in 0..MAX_OPERATIONS {
        println!("--------");

        for member_id in &member_ids {
            let member = members.get_mut(member_id).unwrap();

            if member.is_removed {
                continue;
            }

            let suggestion = member.machine.suggest(INVALID_TRANSITION_CHANCE, &rng);

            if let Operation::Noop = suggestion.operation() {
            } else {
                println!(
                    "member: {}, suggestion: {}",
                    member.machine.values.my_id,
                    suggestion.operation(),
                );
            }

            match &suggestion {
                Suggestion::Valid(operation) => {
                    if let Some(message) = member
                        .next(&operation, &rng)
                        .expect(&format!("valid operations to not fail: {}", operation))
                    {
                        queue.push_back((suggestion.clone(), message));
                    }
                }
                Suggestion::Invalid(operation) => {
                    assert!(
                        member.next(operation, &rng).is_err(),
                        "expected error due to invalid group operation"
                    );
                }
            }
        }

        let mut queue_2nd = queue.clone();
        while let Some((suggestion, _)) = queue_2nd.pop_front() {
            for member_id in &member_ids {
                let member = members.get_mut(member_id).unwrap();
                member.process_operation(&suggestion.operation());
            }
        }

        while let Some((suggestion, message)) = queue.pop_front() {
            println!("msg : {}", message.id());
            for member_id in &member_ids {
                // Do not process our own messages.
                if member_id == &message.sender() {
                    continue;
                }

                let member = members.get_mut(member_id).unwrap();
                match member.process(&message, &rng) {
                    Ok(output) => {
                        if let Suggestion::Invalid(operation) = suggestion {
                            panic!(
                                "expected error when processing remote message from invalid operation '{}'",
                                operation
                            )
                        }

                        member.assert_process(&suggestion.operation(), &output);

                        if let Some(output) = output {
                            for event in output.events {
                                if let message_scheme::GroupEvent::Control(output_message) = event {
                                    queue.push_back((suggestion.clone(), output_message));
                                }
                            }
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
    }
});
