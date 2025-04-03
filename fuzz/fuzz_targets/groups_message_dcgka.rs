// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::collections::{HashMap, VecDeque};

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

#[derive(Debug)]
struct Member {
    is_welcomed: bool,
    my_id: MemberId,
    dcgka: TestDcgkaState,
    inbox: VecDeque<BroadcastMessage>,
    update_secrets: HashMap<(MemberId, MemberId), UpdateSecret>,
    seq: MessageId,
}

#[derive(Clone, Debug)]
struct BroadcastMessage {
    control_message: ControlMessage<MemberId, MessageId>,
    direct_messages: Vec<DirectMessage<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>>,
    sender: MemberId,
    seq: MessageId,
}

impl Member {
    fn create(
        &mut self,
        initial_members: &[MemberId],
        _test: &mut AssertableDcgka,
        rng: &Rng,
    ) -> BroadcastMessage {
        let seq = self.seq.clone();

        let (dcgka_pre, pre) =
            Dcgka::create(self.dcgka.clone(), initial_members.to_vec(), &rng).unwrap();
        let (dcgka_i, output) = Dcgka::process_local(dcgka_pre, seq, pre, &rng).unwrap();

        if let Some(update_secret) = output.me_update_secret {
            println!("- own secret for {}", self.my_id);
            self.update_secrets
                .insert((self.my_id, self.my_id), update_secret);
        }

        // test.assert_create(&dcgka_i, &output, self.my_id, initial_members, seq);

        self.is_welcomed = true;
        self.seq = self.seq.inc();
        self.dcgka = dcgka_i;

        BroadcastMessage {
            control_message: output.control_message,
            direct_messages: output.direct_messages,
            sender: self.my_id,
            seq,
        }
    }

    fn add(&mut self, added: MemberId, _test: &mut AssertableDcgka, rng: &Rng) -> BroadcastMessage {
        let seq = self.seq.clone();

        let (dcgka_pre, pre) = Dcgka::add(self.dcgka.clone(), added, &rng).unwrap();
        let (dcgka_i, output) = Dcgka::process_local(dcgka_pre, seq, pre, &rng).unwrap();

        if let Some(update_secret) = output.me_update_secret {
            println!("- own secret for {}", self.my_id);
            self.update_secrets
                .insert((self.my_id, self.my_id), update_secret);
        }

        // test.assert_add(&dcgka_i, &output, self.my_id, added, seq);

        self.seq = self.seq.inc();
        self.dcgka = dcgka_i;

        BroadcastMessage {
            control_message: output.control_message,
            direct_messages: output.direct_messages,
            sender: self.my_id,
            seq,
        }
    }

    fn update(
        &mut self,
        _expected_members: &[MemberId],
        _test: &mut AssertableDcgka,
        rng: &Rng,
    ) -> BroadcastMessage {
        let seq = self.seq.clone();

        let (dcgka_pre, pre) = Dcgka::update(self.dcgka.clone(), &rng).unwrap();
        let (dcgka_i, output) = Dcgka::process_local(dcgka_pre, seq, pre, &rng).unwrap();

        if let Some(update_secret) = output.me_update_secret {
            println!("- own secret for {}", self.my_id);
            self.update_secrets
                .insert((self.my_id, self.my_id), update_secret);
        }

        // test.assert_update(&dcgka_i, &output, self.my_id, expected_members, seq);

        self.seq = self.seq.inc();
        self.dcgka = dcgka_i;

        BroadcastMessage {
            control_message: output.control_message,
            direct_messages: output.direct_messages,
            sender: self.my_id,
            seq,
        }
    }

    fn receive(&mut self, message: BroadcastMessage) {
        self.inbox.push_back(message);
    }

    fn process(&mut self, _test: &mut AssertableDcgka, rng: &Rng) -> Option<BroadcastMessage> {
        let Some(message) = self.inbox.pop_front() else {
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

        let result = Dcgka::process_remote(self.dcgka.clone(), input.clone(), &rng);
        let Ok((dcgka_i, output)) = result else {
            println!("{:?} my_id={}", input, self.my_id);
            panic!();
        };

        // Ok((dcgka_i, output)) => {
        // match &message.control_message {
        //     ControlMessage::Create(CreateMessage { initial_members }) => test
        //         .assert_process_create(
        //             &dcgka_i,
        //             &output,
        //             self.my_id,
        //             message.sender,
        //             &initial_members,
        //             message.seq,
        //         ),
        //     ControlMessage::Ack(_) => {
        //         test.assert_process_ack(&dcgka_i, &output, self.my_id, message.sender, message.seq)
        //     }
        //     ControlMessage::Update(_) => test.assert_process_update(
        //         &dcgka_i,
        //         &output,
        //         self.my_id,
        //         message.sender,
        //         message.seq,
        //     ),
        //     ControlMessage::Remove(_) => test.assert_process_remove(
        //         &dcgka_i,
        //         &output,
        //         self.my_id,
        //         message.sender,
        //         message.seq,
        //     ),
        //     ControlMessage::Add(AddMessage { added }) => test.assert_process_add(
        //         &dcgka_i,
        //         &output,
        //         self.my_id,
        //         message.sender,
        //         *added,
        //         message.seq,
        //     ),
        //     ControlMessage::AddAck(_) => {
        //         test.assert_process_add_ack(&dcgka_i, &output, self.my_id, message.sender)
        //     }
        // }

        if let ControlMessage::Add(AddMessage { added }) = message.control_message {
            if added == self.my_id {
                self.is_welcomed = true;
            }
        }

        if let ControlMessage::Create(CreateMessage { initial_members }) = &message.control_message
        {
            if initial_members.contains(&self.my_id) {
                self.is_welcomed = true;
            }
        }

        if !self.is_welcomed {
            return None;
        }

        println!(
            "{} processes '{}' message from {} @ seq={}\n    ~> ctrl: {}, dm: [{}]",
            self.my_id,
            message.control_message,
            message.sender,
            message.seq,
            output
                .control_message
                .clone()
                .map(|msg| format!("'{}' [seq={}]", msg.to_string(), self.seq))
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

        self.dcgka = dcgka_i;

        match output.control_message {
            Some(control_message) => {
                let seq = self.seq.clone();
                self.seq = self.seq.inc();
                Some(BroadcastMessage {
                    control_message,
                    direct_messages: output.direct_messages,
                    sender: self.my_id,
                    seq,
                })
            }
            None => None,
        }
    }
}

const GROUP_SIZE: usize = 24;

fuzz_target!(|actions: Vec<Action>| {
    let rng = Rng::from_seed([1; 32]);

    // Initialise
    // ~~~~~~~~~~

    let member_ids = {
        let mut buf = Vec::with_capacity(GROUP_SIZE);
        for i in 0..GROUP_SIZE {
            buf.push(i);
        }
        buf
    };

    let states: [TestDcgkaState; GROUP_SIZE] =
        init_dcgka_state(member_ids.try_into().unwrap(), &rng);

    let mut members = HashMap::new();
    for i in 0..GROUP_SIZE {
        members.insert(
            i,
            Member {
                is_welcomed: false,
                my_id: i,
                dcgka: states[i].clone(),
                inbox: VecDeque::new(),
                update_secrets: HashMap::new(),
                seq: MessageId::new(i),
            },
        );
    }

    let mut test = AssertableDcgka::new();

    // Create group
    // ~~~~~~~~~~~~

    let mut expected_members = vec![0, 1];

    println!();
    println!(
        "* create group (members = [{}])",
        expected_members
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<String>>()
            .join(", ")
    );

    let message = {
        let member = members.get_mut(&0).unwrap();
        member.create(&expected_members, &mut test, &rng)
    };

    for member_id in without(&expected_members, 0) {
        let member = members.get_mut(&member_id).unwrap();
        member.receive(message.clone());
        let message_2 = member.process(&mut test, &rng);

        if let Some(message_2) = message_2 {
            for member_id_2 in &expected_members {
                if *member_id_2 == message_2.sender {
                    continue;
                }

                let member_2 = members.get_mut(&member_id_2).unwrap();
                member_2.receive(message_2.clone());
            }
        }
    }

    for member_id in &expected_members {
        let member = members.get(&member_id).unwrap();
        assert_members_view(
            &member.dcgka,
            &[ExpectedMembers {
                viewer: &expected_members,
                expected: &expected_members,
            }],
        );
    }

    // Random actions
    // ~~~~~~~~~~~~~~

    for action in actions {
        match action {
            Action::Add => {
                let remaining_members: Vec<MemberId> = (0..GROUP_SIZE)
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
                if !adder.is_welcomed {
                    continue;
                }
                let message = adder.add(*added_member_id, &mut test, &rng);

                // Every current member receives the add.
                for member_id in without(&expected_members, *adder_member_id) {
                    let member = members.get_mut(&member_id).unwrap();
                    member.receive(message.clone());
                }

                let member = members.get_mut(&added_member_id).unwrap();
                member.receive(message.clone());

                expected_members.push(*added_member_id);
            }
            Action::Update => {
                // Find a random member to do the update.
                let index = rng.random_array::<1>().unwrap()[0] % (expected_members.len() as u8);
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
                if !member.is_welcomed {
                    continue;
                }
                let message = member.update(&expected_members, &mut test, &rng);

                // Every current member receives the update.
                for member_id in without(&expected_members, *member_id) {
                    let member = members.get_mut(&member_id).unwrap();
                    member.receive(message.clone());
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
                let index = rng.random_array::<1>().unwrap()[0] % (expected_members.len() as u8);
                let member_id = expected_members.get(index as usize).unwrap();

                // Process next message from inbox.
                let member = members.get_mut(&member_id).unwrap();
                let message = member.process(&mut test, &rng);

                // Every current member receives the message.
                if let Some(message) = message {
                    for member_id in without(&expected_members, *member_id) {
                        let member = members.get_mut(&member_id).unwrap();
                        member.receive(message.clone());
                    }
                }
            }
        }
    }

    println!(
        "drain! (members = [{}])",
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
            let result = member.process(&mut test, &rng);
            if let Some(message) = result {
                queue.push_back(message);
            }
        }

        if let Some(message) = queue.pop_front() {
            for member_id in without(&expected_members, message.sender) {
                let member = members.get_mut(&member_id).unwrap();
                member.receive(message.clone());
            }
        }

        let mut all_inboxes_empty = true;
        for member_id in &expected_members {
            let member = members.get(&member_id).unwrap();
            if !member.inbox.is_empty() {
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

fn without(members: &Vec<MemberId>, member: MemberId) -> Vec<MemberId> {
    members
        .iter()
        .filter(|id| *id != &member)
        .cloned()
        .collect()
}
