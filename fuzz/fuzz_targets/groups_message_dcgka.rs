// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Display;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use p2panda_group::test_utils::{
    AckedTestDGM, AssertableDcgka, ExpectedMembers, MemberId, MessageId, assert_members_view,
    init_dcgka_state,
};
use p2panda_group::{
    AddMessage, ControlMessage, CreateMessage, Dcgka, DcgkaState, DirectMessage, KeyManager,
    KeyRegistry, ProcessInput, RemoveMessage, Rng, UpdateSecret,
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
    Remove,
    Process,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct VectorClock {
    all_observed: HashSet<MessageId>,
    dependencies: HashMap<MemberId, MessageId>,
}

impl VectorClock {
    fn add(&mut self, seq: MessageId) {
        self.all_observed.insert(seq);
        self.dependencies.insert(seq.sender, seq);
    }

    fn dependencies(&self) -> Vec<MessageId> {
        self.dependencies.values().cloned().collect()
    }
}

impl Display for VectorClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "dep={{{}}}, all={{{}}}",
            self.dependencies()
                .iter()
                .map(|seq| seq.to_string())
                .collect::<Vec<String>>()
                .join(", "),
            self.all_observed
                .iter()
                .map(|seq| seq.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        )
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
    pub fn is_welcome(&self, my_id: MemberId) -> bool {
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

    pub fn is_remove(&self, my_id: MemberId) -> bool {
        if let ControlMessage::Remove(RemoveMessage { removed }) = &self.control_message {
            return *removed == my_id;
        }
        false
    }
}

#[derive(Debug)]
struct Member {
    /// Id of this member.
    my_id: MemberId,

    /// DCGKA state.
    dcgka: TestDcgkaState,

    /// Learned update secrets `(from) -> (to)` for this member.
    update_secrets: HashMap<(MemberId, MemberId), UpdateSecret>,

    /// Id which will be used for the next message.
    next_seq: MessageId,

    /// Buffer to store messages which have not been "ready-ed" by the orderer.
    pending: HashMap<MessageId, BroadcastMessage>,

    /// Buffer to keep messages until we've joined the group.
    pending_pre_welcomed: VecDeque<BroadcastMessage>,

    /// As soon as we've joined a group we want to remember the point from which it happened, so we
    /// can ignore all messages from before that point.
    welcome_clock: Option<VectorClock>,

    /// Did this member leave the group?
    is_removed: bool,

    /// List of messages we have processed.
    processed: VectorClock,

    /// Messages for this member arrive here in this "inbox" and get buffered until the
    /// dependencies are met.
    ordering: PartialOrder<MessageId, MemoryStore<MessageId>>,
}

impl Member {
    pub fn is_welcomed(&self) -> bool {
        self.welcome_clock.is_some()
    }

    pub fn member_view(&self) -> HashSet<MemberId> {
        Dcgka::member_view(&self.dcgka, &self.my_id).unwrap()
    }

    pub fn create(
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

    pub fn add(
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

    pub fn update(
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

    pub fn remove(
        &mut self,
        removed: MemberId,
        expected_members: &[MemberId],
        test: Option<&mut AssertableDcgka>,
        rng: &Rng,
    ) -> BroadcastMessage {
        let seq = self.next_seq.clone();

        let (dcgka_pre, pre) = Dcgka::remove(self.dcgka.clone(), removed, &rng).unwrap();
        let (dcgka_i, output) = Dcgka::process_local(dcgka_pre, seq, pre, &rng).unwrap();

        if let Some(ref update_secret) = output.me_update_secret {
            println!("    ~> own secret for {}", self.my_id);
            self.update_secrets
                .insert((self.my_id, self.my_id), update_secret.clone());
        }

        if let Some(test) = test {
            test.assert_remove(
                &dcgka_i,
                &output,
                self.my_id,
                removed,
                expected_members,
                seq,
            );
        }

        self.dcgka = dcgka_i;

        if self.my_id == removed {
            self.is_removed = true;
        }

        self.publish(output.control_message, output.direct_messages, seq)
    }

    pub async fn receive(&mut self, message: BroadcastMessage) {
        if !self.is_welcomed() {
            self.pending_pre_welcomed.push_back(message.clone());
            return;
        };

        // Keep the actual message around here until it gets chosen next by the causal orderer.
        self.pending.insert(message.seq, message.clone());

        // Remove ourselves from dependencies, we know we've seen our own messages.
        let previous: Vec<MessageId> = message
            .previous
            .dependencies()
            .into_iter()
            .filter(|msg| msg.sender != self.my_id)
            .collect();

        self.ordering.process(message.seq, &previous).await.unwrap();
    }

    pub async fn process(
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

        // Do not process seen messages again.
        if self.processed.all_observed.contains(&message.seq) {
            return None;
        }

        if let Some(ref welcome_clock) = self.welcome_clock {
            // Remove all messages prior or equal to "welcome".
            if welcome_clock.all_observed.contains(&message.seq) {
                return None;
            }

            // Don't process our welcomes again.
            if message.is_welcome(self.my_id) {
                return None;
            }

            // "create" messages don't have any clock info, so we need to filter them out here
            // explicitly when we haven't been part of the "initial members" set.
            if matches!(message.control_message, ControlMessage::Create(_)) {
                if !message.is_welcome(self.my_id) {
                    return None;
                }
            }
        }

        if message.is_remove(self.my_id) {
            self.is_removed = true;
        }

        if self.is_removed {
            return None;
        }

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
                    "member {} processing '{}' {} failed with: {}\nwelcome: {}\nclock: {}",
                    self.my_id,
                    input.control_message,
                    input.seq,
                    err,
                    self.welcome_clock
                        .as_ref()
                        .map(|clock| clock.to_string())
                        .unwrap_or("none".into()),
                    message.previous,
                );
            });

        // Did processing this message remove ourselves?
        if !Dcgka::member_view(&dcgka_i, &self.my_id)
            .unwrap()
            .contains(&self.my_id)
        {
            self.is_removed = true;
        }

        if self.is_removed {
            return None;
        }

        match &message.control_message {
            ControlMessage::Create(CreateMessage { initial_members }) => {
                if let Some(test) = test {
                    test.assert_process_create(
                        &dcgka_i,
                        &output,
                        self.my_id,
                        message.sender,
                        &initial_members,
                        message.seq,
                    );
                }
            }
            ControlMessage::Ack(_) => {
                if let Some(test) = test {
                    test.assert_process_ack(
                        &dcgka_i,
                        &output,
                        self.my_id,
                        message.sender,
                        message.seq,
                    );
                }
            }
            ControlMessage::Update(_) => {
                if let Some(test) = test {
                    test.assert_process_update(
                        &dcgka_i,
                        &output,
                        self.my_id,
                        message.sender,
                        message.seq,
                    );
                }
            }
            ControlMessage::Remove(_) => {
                if let Some(test) = test {
                    test.assert_process_remove(
                        &dcgka_i,
                        &output,
                        self.my_id,
                        message.sender,
                        message.seq,
                    );
                }
            }
            ControlMessage::Add(AddMessage { added }) => {
                if let Some(test) = test {
                    test.assert_process_add(
                        &dcgka_i,
                        &output,
                        self.my_id,
                        message.sender,
                        *added,
                        message.seq,
                    );
                }
            }
            ControlMessage::AddAck(_) => {
                if let Some(test) = test {
                    test.assert_process_add_ack(&dcgka_i, &output, self.my_id, message.sender)
                }
            }
        }

        println!(
            "{} processes '{}' message {}",
            self.my_id, message.control_message, message.seq,
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
                self.receive(pending).await;
            }
        }

        self.processed.add(message.seq);
        self.dcgka = dcgka_i;

        match output.control_message {
            Some(control_message) => {
                Some(self.publish(control_message, output.direct_messages, self.next_seq))
            }
            None => None,
        }
    }

    pub fn publish(
        &mut self,
        control_message: ControlMessage<MemberId, MessageId>,
        direct_messages: Vec<DirectMessage<MemberId, MessageId, AckedTestDGM<MemberId, MessageId>>>,
        seq: MessageId,
    ) -> BroadcastMessage {
        println!(
            "    ~> ctrl: '{}' {}, dm: [{}]",
            control_message,
            seq,
            direct_messages
                .iter()
                .map(|dm| format!("{}@{}", dm.message_type(), dm.recipient.to_string()))
                .collect::<Vec<String>>()
                .join(", "),
        );

        let previous = self.processed.clone();
        self.processed.add(seq);
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
                    is_removed: false,
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

        let mut global_added_members = INITIAL_MEMBERS.to_vec();
        let mut global_removed_members = vec![];

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
                    // Add a random member.
                    let added_member_id = {
                        let remaining_members: Vec<MemberId> = (0..MAX_GROUP_SIZE)
                            .filter(|id| {
                                !global_added_members.contains(id)
                                    && !global_removed_members.contains(id)
                            })
                            .collect();

                        // We've added all possible members, stop here.
                        if remaining_members.is_empty() {
                            break;
                        }

                        let index =
                            rng.random_array::<1>().unwrap()[0] % (remaining_members.len() as u8);
                        remaining_members.get(index as usize).unwrap().clone()
                    };

                    // Find a random member to do the add.
                    let adder_member_id = {
                        let current_members: Vec<MemberId> = global_added_members
                            .iter()
                            .filter(|id| !global_removed_members.contains(id))
                            .cloned()
                            .collect();
                        let index =
                            rng.random_array::<1>().unwrap()[0] % (current_members.len() as u8);
                        current_members.get(index as usize).unwrap().clone()
                    };

                    println!("* {} adds {}", adder_member_id, added_member_id);

                    // Add operation.
                    let adder = members.get_mut(&adder_member_id).unwrap();
                    if !adder.is_welcomed() {
                        continue;
                    }
                    let message = adder.add(added_member_id, None, &rng);

                    // Every member receives the add.
                    for member_id in without(&max_members, adder_member_id) {
                        let member = members.get_mut(&member_id).unwrap();
                        member.receive(message.clone()).await;
                    }

                    global_added_members.push(added_member_id);
                }
                Action::Update => {
                    // Find a random member to do the update.
                    let member_id = {
                        let current_members: Vec<MemberId> = global_added_members
                            .iter()
                            .filter(|id| !global_removed_members.contains(id))
                            .cloned()
                            .collect();
                        let index =
                            rng.random_array::<1>().unwrap()[0] % (current_members.len() as u8);
                        current_members.get(index as usize).unwrap().clone()
                    };

                    println!("* {} updates", member_id,);

                    // Update operation.
                    let message = {
                        let member = members.get_mut(&member_id).unwrap();
                        if !member.is_welcomed() {
                            continue;
                        }
                        member.update(&[], None, &rng)
                    };

                    // Every current member receives the update.
                    for member_id in without(&max_members, member_id) {
                        let member = members.get_mut(&member_id).unwrap();
                        member.receive(message.clone()).await;
                    }
                }
                Action::Remove => {
                    // We removed almost all members, stop here.
                    if global_removed_members.len() >= global_added_members.len() - 1 {
                        break;
                    }

                    // Find a random member to do the remove (it's possible that the member removes
                    // themselves).
                    let remover_member_id = {
                        let current_members: Vec<MemberId> = global_added_members
                            .iter()
                            .filter(|id| !global_removed_members.contains(id))
                            .cloned()
                            .collect();
                        let index =
                            rng.random_array::<1>().unwrap()[0] % (current_members.len() as u8);
                        current_members.get(index as usize).unwrap().to_owned()
                    };

                    let remover_member = members.get_mut(&remover_member_id).unwrap();
                    if !remover_member.is_welcomed() {
                        continue;
                    }

                    // Find a random member to remove (from the members view of the remover).
                    let removed_member_id = {
                        let member_view: Vec<MemberId> =
                            remover_member.member_view().into_iter().collect();
                        let index = rng.random_array::<1>().unwrap()[0] % (member_view.len() as u8);
                        member_view.get(index as usize).unwrap().to_owned()
                    };

                    println!("* {} removes {}", remover_member_id, removed_member_id);

                    // Remove operation.
                    let message = remover_member.remove(removed_member_id, &[], None, &rng);

                    // Every current member receives the remove.
                    for member_id in without(&max_members, remover_member_id) {
                        let member = members.get_mut(&member_id).unwrap();
                        member.receive(message.clone()).await;
                    }

                    global_removed_members.push(removed_member_id);
                }
                Action::Process => {
                    // Find a random member to do the process.
                    let member_id = {
                        let current_members: Vec<MemberId> = global_added_members
                            .iter()
                            .filter(|id| !global_removed_members.contains(id))
                            .cloned()
                            .collect();
                        let index =
                            rng.random_array::<1>().unwrap()[0] % (current_members.len() as u8);
                        current_members.get(index as usize).unwrap().clone()
                    };

                    // Process next message from inbox.
                    let message = {
                        let member = members.get_mut(&member_id).unwrap();
                        member.process(None, &rng).await
                    };

                    // Every current member receives the message.
                    if let Some(message) = message {
                        for member_id in without(&max_members, member_id) {
                            let member = members.get_mut(&member_id).unwrap();
                            member.receive(message.clone()).await;
                        }
                    }
                }
            }
        }

        println!("====== process remaining messages! ======");

        // Process all messages
        // ~~~~~~~~~~~~~~~~~~~~

        let mut queue = VecDeque::new();

        let current_members: Vec<MemberId> = global_added_members
            .iter()
            .filter(|id| !global_removed_members.contains(id))
            .cloned()
            .collect();

        loop {
            for member_id in &current_members {
                let member = members.get_mut(&member_id).unwrap();
                let result = member.process(None, &rng).await;
                if let Some(message) = result {
                    queue.push_back(message);
                }
            }

            if let Some(message) = queue.pop_front() {
                for member_id in without(&current_members, message.sender) {
                    let member = members.get_mut(&member_id).unwrap();
                    member.receive(message.clone()).await;
                }
            }

            let mut all_inboxes_empty = true;
            for member_id in &current_members {
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

        for from_id in &current_members {
            let from = members.get(&from_id).unwrap();
            for to_id in &current_members {
                let to = members.get(&to_id).unwrap();

                // Do these members still want to talk to each other? If not their secrets might be
                // out of sync as they seized processing each other's messages at some point.
                if !Dcgka::member_view(&from.dcgka, &from.my_id)
                    .unwrap()
                    .contains(to_id)
                {
                    continue;
                }

                if !Dcgka::member_view(&to.dcgka, &to.my_id)
                    .unwrap()
                    .contains(from_id)
                {
                    continue;
                }

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
