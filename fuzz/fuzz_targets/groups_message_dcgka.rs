// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::collections::{HashMap, VecDeque};
use std::fmt::Display;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use p2panda_group::test_utils::{
    AckedTestDGM, AssertableDcgka, ExpectedMembers, MemberId, MessageId, assert_members_view,
    init_dcgka_state,
};
use p2panda_group::{
    AddMessage, ControlMessage, CreateMessage, Dcgka, DcgkaState, DirectMessage, KeyManager,
    KeyRegistry, ProcessInput, Rng, UpdateSecret,
};
use p2panda_stream::partial::{MemoryStore, PartialOrder};

type TestDcgkaState = DcgkaState<
    MemberId,
    MessageId,
    KeyRegistry<MemberId>,
    AckedTestDGM<MemberId, MessageId>,
    KeyManager,
>;

#[derive(Debug, Arbitrary)]
enum Action {
    Add,
    Update,
    // Remove,
    Process,
}

#[derive(Clone, Default, Debug)]
struct VectorClock(HashMap<MemberId, MessageId>);

impl VectorClock {
    fn get(&self, sender: &MemberId) -> usize {
        self.0.get(sender).map(|msg| msg.seq).unwrap_or(0)
    }

    fn set(&mut self, seq: MessageId) -> Option<MessageId> {
        self.0.insert(seq.sender, seq)
    }

    fn dependencies(&self) -> Vec<MessageId> {
        self.0.values().cloned().collect()
    }
}

impl Display for VectorClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.dependencies()
                .iter()
                .map(|bla| bla.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        )
    }
}

#[derive(Debug)]
struct Member {
    my_id: MemberId,
    dcgka: TestDcgkaState,
    update_secrets: HashMap<(MemberId, MemberId), UpdateSecret>,
    next_seq: MessageId,
    pending: HashMap<MessageId, BroadcastMessage>,
    pending_pre_welcomed: VecDeque<BroadcastMessage>,
    welcome_clock: Option<VectorClock>,
    processed: VectorClock,
    ordering: PartialOrder<MessageId, MemoryStore<MessageId>>,
}

impl Member {
    fn is_welcomed(&self) -> bool {
        self.welcome_clock.is_some()
    }
}

#[derive(Clone, Debug)]
struct BroadcastMessage {
    control_message: ControlMessage<MemberId, MessageId>,
    direct_messages: Vec<DirectMessage<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>>,
    previous: VectorClock,
    sender: MemberId,
    seq: MessageId,
}

impl PartialEq for BroadcastMessage {
    fn eq(&self, other: &Self) -> bool {
        self.seq.eq(&other.seq)
    }
}

impl BroadcastMessage {
    fn is_welcome(&self, my_id: MemberId) -> bool {
        if let ControlMessage::Add(AddMessage { added }) = self.control_message {
            if added == my_id {
                return true;
            }
        }

        if let ControlMessage::Create(CreateMessage { initial_members }) = &self.control_message {
            if initial_members.contains(&my_id) {
                return true;
            }
        }

        false
    }
}

impl Member {
    fn create(
        &mut self,
        initial_members: &[MemberId],
        test: Option<&mut AssertableDcgka>,
        rng: &Rng,
    ) -> BroadcastMessage {
        let seq = self.next_seq.clone();

        let (dcgka_pre, pre) =
            Dcgka::create(self.dcgka.clone(), initial_members.to_vec(), &rng).unwrap();
        let (dcgka_i, output) = Dcgka::process_local(dcgka_pre, seq, pre, &rng).unwrap();

        if let Some(ref update_secret) = output.me_update_secret {
            println!("    ~> own secret for {}", self.my_id);
            self.update_secrets
                .insert((self.my_id, self.my_id), update_secret.clone());
        }

        if let Some(test) = test {
            test.assert_create(&dcgka_i, &output, self.my_id, initial_members, seq);
        }

        self.dcgka = dcgka_i;

        let result = self.publish(output.control_message, output.direct_messages, seq);

        self.welcome_clock = Some(self.processed.clone());

        result
    }

    fn add(
        &mut self,
        added: MemberId,
        test: Option<&mut AssertableDcgka>,
        rng: &Rng,
    ) -> BroadcastMessage {
        let seq = self.next_seq.clone();

        let (dcgka_pre, pre) = Dcgka::add(self.dcgka.clone(), added, &rng).unwrap();
        let (dcgka_i, output) = Dcgka::process_local(dcgka_pre, seq, pre, &rng).unwrap();

        if let Some(ref update_secret) = output.me_update_secret {
            println!("    ~> own secret for {}", self.my_id);
            self.update_secrets
                .insert((self.my_id, self.my_id), update_secret.clone());
        }

        if let Some(test) = test {
            test.assert_add(&dcgka_i, &output, self.my_id, added, seq);
        }

        self.dcgka = dcgka_i;

        self.publish(output.control_message, output.direct_messages, seq)
    }

    fn update(
        &mut self,
        expected_members: &[MemberId],
        test: Option<&mut AssertableDcgka>,
        rng: &Rng,
    ) -> BroadcastMessage {
        let seq = self.next_seq.clone();

        let (dcgka_pre, pre) = Dcgka::update(self.dcgka.clone(), &rng).unwrap();
        let (dcgka_i, output) = Dcgka::process_local(dcgka_pre, seq, pre, &rng).unwrap();

        if let Some(ref update_secret) = output.me_update_secret {
            println!("    ~> own secret for {}", self.my_id);
            self.update_secrets
                .insert((self.my_id, self.my_id), update_secret.clone());
        }

        if let Some(test) = test {
            test.assert_update(&dcgka_i, &output, self.my_id, expected_members, seq);
        }

        self.dcgka = dcgka_i;

        self.publish(output.control_message, output.direct_messages, seq)
    }

    async fn receive(&mut self, message: BroadcastMessage) {
        let Some(ref welcome_clock) = self.welcome_clock else {
            // We're not welcomed yet, so let's keep all messages in a buffer for now until we've
            // entered.
            self.pending_pre_welcomed.push_back(message.clone());
            return;
        };

        let previous = message.previous.clone();

        // TODO
        // Remove ourselves from the dependencies, we know we have seen our own messages.
        // let previous: Vec<MessageId> = message
        //     .previous
        //     .into_iter()
        //     .filter(|msg| msg.sender != self.my_id)
        //     .collect();

        // TODO
        // Remove all messages prior to "welcome".
        if welcome_clock.get(&message.sender) <= message.previous.get(&message.sender) {
            println!(
                "  {} ignores '{}' of {}: welcome_clock = {{{}}}, message.previous = {{{}}}",
                self.my_id,
                message.control_message,
                message.sender,
                welcome_clock,
                message.previous
            );
            return;
        }

        // Keep the actual message around here until it gets chosen next by the causal orderer.
        self.pending.insert(message.seq, message.clone());

        println!(
            "    -- {} queues '{}' {} of {} deps={{{}}}",
            self.my_id, message.control_message, message.seq, message.sender, previous
        );

        self.ordering
            .process(message.seq, &previous.dependencies())
            .await
            .unwrap();
    }

    async fn process(
        &mut self,
        test: Option<&mut AssertableDcgka>,
        rng: &Rng,
    ) -> Option<BroadcastMessage> {
        let next_message = if !self.is_welcomed() {
            // Check if we finally have a welcome message for us.
            self.pending_pre_welcomed
                .iter()
                .find(|message| message.is_welcome(self.my_id))
                .cloned()
        } else {
            match self.ordering.next().await.unwrap() {
                Some(message_id) => {
                    let message = self.pending.remove(&message_id).unwrap();

                    Some(message)
                }
                None => None,
            }
        };

        let Some(message) = next_message else {
            return None;
        };

        let direct_message = message
            .direct_messages
            .iter()
            .find(|message| message.recipient == self.my_id)
            .cloned();

        let input = ProcessInput {
            seq: message.seq,
            sender: message.sender,
            control_message: message.control_message.clone(),
            direct_message: direct_message.clone(),
        };

        let (dcgka_i, output) = Dcgka::process_remote(self.dcgka.clone(), input.clone(), &rng)
            .unwrap_or_else(|err| {
                panic!(
                    "member {} processing '{}' {} failed with: {}",
                    self.my_id, input.control_message, input.seq, err
                );
            });

        if let Some(test) = test {
            match &message.control_message {
                ControlMessage::Create(CreateMessage { initial_members }) => test
                    .assert_process_create(
                        &dcgka_i,
                        &output,
                        self.my_id,
                        message.sender,
                        &initial_members,
                        message.seq,
                    ),
                ControlMessage::Ack(_) => test.assert_process_ack(
                    &dcgka_i,
                    &output,
                    self.my_id,
                    message.sender,
                    message.seq,
                ),
                ControlMessage::Update(_) => test.assert_process_update(
                    &dcgka_i,
                    &output,
                    self.my_id,
                    message.sender,
                    message.seq,
                ),
                ControlMessage::Remove(_) => test.assert_process_remove(
                    &dcgka_i,
                    &output,
                    self.my_id,
                    message.sender,
                    message.seq,
                ),
                ControlMessage::Add(AddMessage { added }) => test.assert_process_add(
                    &dcgka_i,
                    &output,
                    self.my_id,
                    message.sender,
                    *added,
                    message.seq,
                ),
                ControlMessage::AddAck(_) => {
                    test.assert_process_add_ack(&dcgka_i, &output, self.my_id, message.sender)
                }
            }
        }

        println!(
            "{} processes '{}' message {}\n    ~> ctrl: {}, dm: [{}]",
            self.my_id,
            message.control_message,
            message.seq,
            output
                .control_message
                .clone()
                .map(|msg| format!("'{}' {}", msg.to_string(), self.next_seq))
                .unwrap_or(String::from("none")),
            output
                .direct_messages
                .iter()
                .map(|dm| format!("{}@{}", dm.message_type(), dm.recipient.to_string()))
                .collect::<Vec<String>>()
                .join(", "),
        );

        if let Some(message) = direct_message {
            println!("    ~> with '{}' direct message", message.message_type());
        }

        if !output.direct_messages.is_empty() {
            assert!(output.control_message.is_some())
        }

        if let Some(update_secret) = output.me_update_secret {
            println!("    ~> own secret for {}", self.my_id);
            self.update_secrets
                .insert((self.my_id, self.my_id), update_secret);
        }

        if let Some(update_secret) = output.sender_update_secret {
            println!("    ~> secret for {} -> {}", self.my_id, message.sender);
            self.update_secrets
                .insert((self.my_id, message.sender), update_secret);
        }

        if message.is_welcome(self.my_id) {
            self.welcome_clock = Some(message.previous.clone());

            // We can finally use the orderer to now process all messages we couldn't look at
            // before entering the group.
            let pending_pre_welcomed: Vec<BroadcastMessage> =
                self.pending_pre_welcomed.drain(0..).collect();

            for pending in pending_pre_welcomed {
                // Don't re-queue the "welcome" message we've just processed.
                if pending == message {
                    continue;
                }

                self.receive(pending).await;
            }
        }

        self.processed.set(message.seq);
        self.dcgka = dcgka_i;

        match output.control_message {
            Some(control_message) => {
                Some(self.publish(control_message, output.direct_messages, self.next_seq))
            }
            None => None,
        }
    }

    fn publish(
        &mut self,
        control_message: ControlMessage<MemberId, MessageId>,
        direct_messages: Vec<DirectMessage<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>>,
        seq: MessageId,
    ) -> BroadcastMessage {
        let previous = self.processed.clone();
        self.processed.set(seq);
        self.next_seq = self.next_seq.inc();
        BroadcastMessage {
            control_message,
            direct_messages,
            previous,
            sender: self.my_id,
            seq,
        }
    }
}

const MAX_GROUP_SIZE: usize = 24;

const CREATOR_ID: MemberId = 0;

const INITIAL_MEMBERS: [MemberId; 2] = [0, 1];

fuzz_target!(|actions: Vec<Action>| {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    rt.block_on(async {
        let rng = Rng::from_seed([1; 32]);

        // Initialise
        // ~~~~~~~~~~

        let max_members = {
            let mut buf = Vec::with_capacity(MAX_GROUP_SIZE);
            for i in 0..MAX_GROUP_SIZE {
                buf.push(i);
            }
            buf
        };

        let states: [TestDcgkaState; MAX_GROUP_SIZE] =
            init_dcgka_state(max_members.clone().try_into().unwrap(), &rng);

        let mut members = HashMap::new();
        for i in 0..MAX_GROUP_SIZE {
            members.insert(
                i,
                Member {
                    my_id: i,
                    dcgka: states[i].clone(),
                    welcome_clock: None,
                    update_secrets: HashMap::new(),
                    next_seq: MessageId::new(i),
                    pending: HashMap::new(),
                    pending_pre_welcomed: VecDeque::new(),
                    processed: VectorClock::default(),
                    ordering: PartialOrder::new(MemoryStore::default()),
                },
            );
        }

        // Create group
        // ~~~~~~~~~~~~

        let mut expected_members = INITIAL_MEMBERS.to_vec();

        println!();
        println!(
            "* create group (members = [{}])",
            INITIAL_MEMBERS
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        );

        let message = {
            let member = members.get_mut(&CREATOR_ID).unwrap();
            member.create(&INITIAL_MEMBERS, None, &rng)
        };

        for member_id in without(&max_members, CREATOR_ID) {
            let process_response = {
                let member = members.get_mut(&member_id).unwrap();
                member.receive(message.clone()).await;
                member.process(None, &rng).await
            };

            if let Some(process_response) = process_response {
                for member_id_2 in without(&max_members, process_response.sender) {
                    let member_2 = members.get_mut(&member_id_2).unwrap();
                    member_2.receive(process_response.clone()).await;
                }
            }
        }

        for member_id in &INITIAL_MEMBERS {
            let member = members.get(&member_id).unwrap();
            assert_members_view(
                &member.dcgka,
                &[ExpectedMembers {
                    viewer: &INITIAL_MEMBERS,
                    expected: &INITIAL_MEMBERS,
                }],
            );
        }

        // Random actions
        // ~~~~~~~~~~~~~~

        for action in actions {
            match action {
                Action::Add => {
                    let remaining_members: Vec<MemberId> = (0..MAX_GROUP_SIZE)
                        .filter(|id| !expected_members.contains(id))
                        .collect();

                    // We added all members, stop here.
                    if remaining_members.is_empty() {
                        break;
                    }

                    // Add a random member.
                    let added_member_id = {
                        let index =
                            rng.random_array::<1>().unwrap()[0] % (remaining_members.len() as u8);
                        remaining_members.get(index as usize).unwrap()
                    };

                    // Find a random member to do the add.
                    let adder_member_id = {
                        let index =
                            rng.random_array::<1>().unwrap()[0] % (expected_members.len() as u8);
                        expected_members.get(index as usize).unwrap()
                    };

                    println!(
                        "* {} adds {} (members = [{}])",
                        adder_member_id,
                        added_member_id,
                        expected_members
                            .iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    );

                    // Add operation.
                    let adder = members.get_mut(&adder_member_id).unwrap();
                    if !adder.is_welcomed() {
                        continue;
                    }
                    let message = adder.add(*added_member_id, None, &rng);

                    // Every member receives the add.
                    for member_id in without(&max_members, *adder_member_id) {
                        let member = members.get_mut(&member_id).unwrap();
                        member.receive(message.clone()).await;
                    }

                    expected_members.push(*added_member_id);
                }
                Action::Update => {
                    // Find a random member to do the update.
                    let index =
                        rng.random_array::<1>().unwrap()[0] % (expected_members.len() as u8);
                    let member_id = expected_members.get(index as usize).unwrap();

                    println!(
                        "* {} updates (members = [{}])",
                        member_id,
                        expected_members
                            .iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    );

                    // Update operation.
                    let member = members.get_mut(&member_id).unwrap();
                    if !member.is_welcomed() {
                        continue;
                    }
                    let message = member.update(&expected_members, None, &rng);

                    // Every current member receives the update.
                    for member_id in without(&max_members, *member_id) {
                        let member = members.get_mut(&member_id).unwrap();
                        member.receive(message.clone()).await;
                    }
                }
                // TODO
                // Action::Remove => {
                //     // We removed all members, stop here.
                //     if expected_members.len() == 1 {
                //         break;
                //     }
                //
                //     // Remove a random member.
                //     let index = rng.random_array::<1>().unwrap()[0] % (expected_members.len() as u8);
                //     let member_id = expected_members.get(index as usize).unwrap();
                //     expected_members = expected_members
                //         .iter()
                //         .filter(|id| *id != member_id)
                //         .cloned()
                //         .collect();
                // }
                Action::Process => {
                    // Find a random member to do the process.
                    let index =
                        rng.random_array::<1>().unwrap()[0] % (expected_members.len() as u8);
                    let member_id = expected_members.get(index as usize).unwrap();

                    // Process next message from inbox.
                    let member = members.get_mut(&member_id).unwrap();
                    let message = member.process(None, &rng).await;

                    // Every current member receives the message.
                    if let Some(message) = message {
                        for member_id in without(&max_members, *member_id) {
                            let member = members.get_mut(&member_id).unwrap();
                            member.receive(message.clone()).await;
                        }
                    }
                }
            }
        }

        println!(
            "====== drain! (members = [{}]) ======",
            expected_members
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        );

        // Process all messages
        // ~~~~~~~~~~~~~~~~~~~~

        let mut queue = VecDeque::new();

        loop {
            for member_id in &expected_members {
                let member = members.get_mut(&member_id).unwrap();
                let result = member.process(None, &rng).await;
                if let Some(message) = result {
                    queue.push_back(message);
                }
            }

            if let Some(message) = queue.pop_front() {
                for member_id in without(&expected_members, message.sender) {
                    let member = members.get_mut(&member_id).unwrap();
                    member.receive(message.clone()).await;
                }
            }

            let mut all_inboxes_empty = true;
            for member_id in &expected_members {
                let member = members.get(&member_id).unwrap();
                if !member.pending.is_empty() {
                    all_inboxes_empty = false;
                }
            }

            if all_inboxes_empty && queue.is_empty() {
                break;
            }
        }

        // Verify
        // ~~~~~~

        for from_id in &expected_members {
            let from = members.get(&from_id).unwrap();
            for to_id in &expected_members {
                let to = members.get(&to_id).unwrap();
                assert_eq!(
                    from.update_secrets
                        .get(&(*from_id, *to_id))
                        .expect(&format!("update secret: {} -> {}", from_id, to_id)),
                    to.update_secrets
                        .get(&(*to_id, *to_id))
                        .expect(&format!("update secret: {} -> {}", to_id, to_id)),
                    "update secrets not matching: {} -> {}",
                    from_id,
                    to_id
                );
            }
        }
    });
});

fn without(members: &Vec<MemberId>, member: MemberId) -> Vec<MemberId> {
    members
        .iter()
        .filter(|id| *id != &member)
        .cloned()
        .collect()
}
