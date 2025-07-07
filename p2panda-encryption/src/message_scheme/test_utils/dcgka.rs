// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};

use crate::Rng;
use crate::crypto::x25519::SecretKey;
use crate::key_bundle::Lifetime;
use crate::key_manager::KeyManager;
use crate::key_registry::KeyRegistry;
use crate::message_scheme::dcgka::{
    ControlMessage, Dcgka, DcgkaState, DirectMessage, DirectMessageType, OperationOutput,
    ProcessOutput, UpdateSecret,
};
use crate::message_scheme::test_utils::dgm::AckedTestDgm;
use crate::test_utils::{MemberId, MessageId};
use crate::traits::{AckedGroupMembership, PreKeyManager};

pub type TestDcgkaState = DcgkaState<
    MemberId,
    MessageId,
    KeyRegistry<MemberId>,
    AckedTestDgm<MemberId, MessageId>,
    KeyManager,
>;

/// Helper method returning initialised DCGKA state for each member of a test group.
///
/// The method will automatically generate all required one-time pre-key bundles from each member
/// and register them for each other.
pub fn init_dcgka_state<const N: usize>(
    member_ids: [MemberId; N],
    rng: &Rng,
) -> [TestDcgkaState; N] {
    let mut key_bundles = HashMap::new();
    let mut key_managers = HashMap::new();

    // Generate a pre-key bundle for each other member of the group.
    for id in member_ids {
        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let mut manager = KeyManager::init(&identity_secret, Lifetime::default(), rng).unwrap();

        let mut bundle_list = Vec::with_capacity(member_ids.len());
        for _ in member_ids {
            let (manager_i, key_bundle) =
                KeyManager::generate_onetime_bundle(manager, rng).unwrap();
            bundle_list.push(key_bundle);
            manager = manager_i;
        }

        key_bundles.insert(id, bundle_list);
        key_managers.insert(id, manager);
    }

    // Register each other's pre-key bundles and initialise DCGKA state.
    let mut result = Vec::with_capacity(member_ids.len());
    for id in member_ids {
        let dgm = AckedTestDgm::init(id);
        let registry = {
            let mut state = KeyRegistry::init();
            for bundle_id in member_ids {
                let bundle = key_bundles.get_mut(&bundle_id).unwrap().pop().unwrap();
                let state_i = KeyRegistry::add_onetime_bundle(state, bundle_id, bundle);
                state = state_i;
            }
            state
        };
        let manager = key_managers.remove(&id).unwrap();
        let dcgka: TestDcgkaState = Dcgka::init(id, manager, registry, dgm);
        result.push(dcgka);
    }

    result.try_into().unwrap()
}

fn members_without(members: &[MemberId], without: &[MemberId]) -> Vec<MemberId> {
    members
        .iter()
        .filter(|id| !without.contains(id))
        .cloned()
        .collect()
}

pub fn assert_direct_message(
    direct_messages: &[DirectMessage<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>],
    recipient: MemberId,
) -> DirectMessage<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>> {
    direct_messages
        .iter()
        .find(|message| message.recipient == recipient)
        .cloned()
        .unwrap_or_else(|| panic!("could not find direct message for {:?}", recipient))
        .clone()
}

pub struct ExpectedMembers<'a> {
    pub viewer: &'a [MemberId],
    pub expected: &'a [MemberId],
}

pub fn assert_members_view(dcgka: &TestDcgkaState, assertions: &[ExpectedMembers]) {
    for assertion in assertions {
        for viewer in assertion.viewer {
            assert_eq!(
                AckedTestDgm::members_view(&dcgka.dgm, viewer).unwrap(),
                HashSet::from_iter(assertion.expected.iter().cloned()),
                "{} should have had members view {:?}",
                viewer,
                assertion.expected
            );
        }
    }
}

/// Testing helper to verify DCGKA group operations and states.
pub struct AssertableDcgka {
    /// Update secrets the DCGKA exported for "local member -> remote member".
    update_secrets: HashMap<(MemberId, MemberId), UpdateSecret>,
}

impl Default for AssertableDcgka {
    fn default() -> Self {
        Self::new()
    }
}

impl AssertableDcgka {
    pub fn new() -> Self {
        Self {
            update_secrets: HashMap::new(),
        }
    }

    /// Expected local state after a member created a group.
    pub fn assert_create(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &OperationOutput<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>,
        creator_id: MemberId,          // Group "creator"
        expected_members: &[MemberId], // List of expected initial group members
        seq: MessageId,                // Id of "create" control message
    ) {
        // This is a local group operation and the group "creator" manages that state.
        assert_eq!(dcgka.my_id, creator_id);

        // Control messages
        // ~~~~~~~~~~~~~~~~

        // Group "creator" broadcasts a "create" control message to everyone.
        let ControlMessage::Create {
            ref initial_members,
        } = output.control_message
        else {
            panic!("expected \"create\" control message");
        };
        assert_eq!(initial_members, expected_members);

        // Direct messages
        // ~~~~~~~~~~~~~~~

        // Group "creator" sends direct 2SM messages to each other member of the group.
        assert_eq!(output.direct_messages.len(), expected_members.len() - 1);
        for (index, expected_member) in members_without(expected_members, &[creator_id])
            .iter()
            .enumerate()
        {
            assert_eq!(
                output.direct_messages.get(index).unwrap().message_type(),
                DirectMessageType::TwoParty,
            );
            assert_eq!(
                output.direct_messages.get(index).unwrap().recipient,
                *expected_member,
            );
        }

        // Members view
        // ~~~~~~~~~~~~

        // Group "creator" considers that all members are part of the group now and every member
        // has processed the "create" control message.
        assert_members_view(
            dcgka,
            &[ExpectedMembers {
                viewer: expected_members,
                expected: expected_members,
            }],
        );

        // Update Secrets
        // ~~~~~~~~~~~~~~

        // Group "creator" establishes the update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // Remember group "creator's" update secret for later assertions.
        self.update_secrets.insert(
            (creator_id, creator_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Key Material
        // ~~~~~~~~~~~~

        // Seed secret has been dropped after group got created (FS).
        assert!(dcgka.next_seed.is_none());

        // Group "creator" established member secrets for all expected members of the group.
        assert_eq!(dcgka.member_secrets.len(), expected_members.len() - 1);
        for member_id in members_without(expected_members, &[creator_id]) {
            assert!(
                dcgka
                    .member_secrets
                    .contains_key(&(creator_id, seq, member_id))
            );
        }

        // Outer-Ratchet holds only the secret for the group "creator" so far.
        assert_eq!(dcgka.ratchet.len(), 1);
        assert!(dcgka.ratchet.contains_key(&creator_id));
    }

    /// Expected local state after an invited member processed a "create" control message.
    pub fn assert_process_create(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>,
        processor_id: MemberId, // "Processor" who handles "create" control message
        creator_id: MemberId,   // Group "creator"
        expected_members: &[MemberId], // List of expected members after processing "create"
        seq: MessageId,         // Id of "create" control message
    ) {
        // We're looking at the state of the "processor".
        assert_eq!(dcgka.my_id, processor_id);
        assert_ne!(creator_id, processor_id);

        // Control messages
        // ~~~~~~~~~~~~~~~~

        // "Processing" member of "create" message broadcasts an "ack" control message to everyone.
        let Some(ControlMessage::Ack {
            ack_sender,
            ack_seq,
        }) = output.control_message
        else {
            panic!("expected \"ack\" control message");
        };

        // "Processing" member acknowledges the "create" message of the creator.
        assert_eq!(ack_sender, creator_id);
        assert_eq!(ack_seq, seq);

        // Direct messages
        // ~~~~~~~~~~~~~~~

        // No direct messages.
        assert!(output.direct_messages.is_empty());

        // Members view
        // ~~~~~~~~~~~~

        // "Processor" of "create" considers all members part of the group now and every member has
        // processed the "create" control message.
        assert_members_view(
            dcgka,
            &[ExpectedMembers {
                viewer: expected_members,
                expected: expected_members,
            }],
        );

        // Update Secrets
        // ~~~~~~~~~~~~~~

        // "Processor" establishes the update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // Processor establishes the update secret for creator's message ratchet.
        assert!(output.sender_update_secret.is_some());

        // Remember "processor's" update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, processor_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Remember "creator's" update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, creator_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // Processor should be aware now of creator's update secret.
        self.assert_update_secrets(processor_id, creator_id);

        // Key Material
        // ~~~~~~~~~~~~

        // Seed was never used and should be none.
        assert!(dcgka.next_seed.is_none());

        // When joining a group freshly we have member secrets for every member who is not the
        // "creator".
        assert_eq!(
            dcgka.member_secrets.len(),
            members_without(expected_members, &[creator_id, processor_id]).len()
        );

        // Outer-Ratchet holds only the secret for the group "creator" and ourselves ("processor") so far.
        assert_eq!(dcgka.ratchet.len(), 2);
        assert!(dcgka.ratchet.contains_key(&creator_id));
        assert!(dcgka.ratchet.contains_key(&processor_id));
    }

    /// Expected local state after a member processed an "ack" control message.
    pub fn assert_process_ack(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>,
        processor_id: MemberId, // Member who "processes" the "ack" control message
        acker_id: MemberId,     // Author of the "ack" control message,
        seq: MessageId,         // Id of the "ack" message
    ) {
        // We're looking at the state of the "processor".
        assert_eq!(dcgka.my_id, processor_id);
        assert_ne!(acker_id, processor_id);

        // Control messages
        // ~~~~~~~~~~~~~~~~

        // No control messages.
        assert!(output.control_message.is_none());

        // Direct messages
        // ~~~~~~~~~~~~~~~

        // No direct messages.
        assert!(output.direct_messages.is_empty());

        // Update Secrets
        // ~~~~~~~~~~~~~~

        // No new update secret for acking member.
        assert!(output.me_update_secret.is_none());

        // Processor establishes the update secret for acking member's message ratchet.
        assert!(output.sender_update_secret.is_some());

        // Remember ackers's update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, acker_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // Processor should be aware now of acker's update secret.
        self.assert_update_secrets(processor_id, acker_id);

        // Key Material
        // ~~~~~~~~~~~~

        // Seed was never used and should be none.
        assert!(dcgka.next_seed.is_none());

        // Member secrets for "acker" has to be removed (FS).
        assert!(
            !dcgka
                .member_secrets
                .contains_key(&(acker_id, seq, processor_id))
        );

        // Outer-Ratchet holds secrets for at least the "acker" and "processor" of the "ack".
        assert!(dcgka.ratchet.contains_key(&acker_id));
        assert!(dcgka.ratchet.contains_key(&processor_id));
    }

    /// Expected local state after an member was added to the group.
    pub fn assert_add(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &OperationOutput<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>,
        adder_id: MemberId, // "Adder" who adds someone to the group
        added_id: MemberId, // "Added" who will join the group
        seq: MessageId,     // Id of the "add" control message
    ) {
        // This is a local group operation, so we expect this to be the "adder".
        assert_eq!(dcgka.my_id, adder_id);
        assert_ne!(adder_id, added_id);

        // Control messages
        // ~~~~~~~~~~~~~~~~

        // "Adder" broadcasts an "add" control message to everyone.
        let ControlMessage::Add { added } = output.control_message else {
            panic!("expected \"add\" control message");
        };
        assert_eq!(added, added_id);

        // Direct messages
        // ~~~~~~~~~~~~~~~

        // One direct "welcome" message to "added" was generated.
        assert_eq!(output.direct_messages.len(), 1);
        assert_eq!(
            output.direct_messages.first().unwrap().message_type(),
            DirectMessageType::Welcome
        );
        assert_eq!(output.direct_messages.first().unwrap().recipient, added_id);

        // Update Secrets
        // ~~~~~~~~~~~~~~

        // "Adder" establishes a new update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // Remember "adders's" update secret for later assertions.
        let previous = self.update_secrets.insert(
            (adder_id, adder_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // The new update secret does not match the previous one.
        assert_ne!(
            previous.unwrap(),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Key Material
        // ~~~~~~~~~~~~

        // Seed was never used and should be none.
        assert!(dcgka.next_seed.is_none());

        // Member secret for the "added" was established.
        assert!(
            dcgka
                .member_secrets
                .contains_key(&(adder_id, seq, added_id))
        );

        // Outer-Ratchet holds secrets for at least the "adder".
        assert!(dcgka.ratchet.contains_key(&adder_id));

        // The added doesn't have a ratchet secret yet.
        assert!(!dcgka.ratchet.contains_key(&added_id));
    }

    /// Expected local state after an invited member "added" processes the "add" message with a
    /// direct "welcome" message addressing them.
    pub fn assert_process_welcome(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>,
        adder_id: MemberId,            // Member who invited to the group
        added_id: MemberId,            // Member who was added to the group
        expected_members: &[MemberId], // List of expected members after processing "add"
        seq: MessageId,                // Id of the "add" control message
    ) {
        // This control message is processed by the member who was added.
        assert_eq!(dcgka.my_id, added_id);
        assert_ne!(adder_id, added_id);

        // Control messages
        // ~~~~~~~~~~~~~~~~

        // Added broadcasts an "ack" control message to everyone, no direct messages.
        let Some(ControlMessage::Ack {
            ack_sender,
            ack_seq,
        }) = output.control_message
        else {
            panic!("expected \"ack\" control message");
        };

        // "Added" acknowledges the "add" message of "adder".
        assert_eq!(ack_sender, adder_id);
        assert_eq!(ack_seq, seq);

        // Direct messages
        // ~~~~~~~~~~~~~~~

        // No direct messages.
        assert!(output.direct_messages.is_empty());

        // Members view
        // ~~~~~~~~~~~~

        // "Added" considers all members as part of the group now and that "adder" has the same
        // view as them.
        assert_members_view(
            dcgka,
            &[ExpectedMembers {
                viewer: &[added_id, adder_id],
                expected: expected_members,
            }],
        );

        // Update Secrets
        // ~~~~~~~~~~~~~~

        // Remember "added's" update secret for later assertions.
        self.update_secrets.insert(
            (added_id, added_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Remember "adder's" update secret for later assertions.
        self.update_secrets.insert(
            (added_id, adder_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // "Added" should be aware now of "adder's" update secret.
        self.assert_update_secrets(added_id, adder_id);

        // Key Material
        // ~~~~~~~~~~~~

        // Seed was never used and should be none.
        assert!(dcgka.next_seed.is_none());

        // Member secret for the "added" was dropped (FS).
        assert!(
            !dcgka
                .member_secrets
                .contains_key(&(adder_id, seq, added_id))
        );

        // Outer-Ratchet holds secrets for at least the "adder" and "added".
        assert!(dcgka.ratchet.contains_key(&adder_id));
        assert!(dcgka.ratchet.contains_key(&added_id));
    }

    /// Expected local state after a member who is _not_ invited processes an "add" control
    /// message.
    #[allow(clippy::too_many_arguments)]
    pub fn assert_process_add(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>,
        processor_id: MemberId, // "Processor" of the "add" control message
        adder_id: MemberId,     // Id of the member who invited the new member
        added_id: MemberId,     // Id of the member which got added
        seq: MessageId,         // Id of the "add" message which is processed
    ) {
        // This control message is processed by every member who is _not_ the "added" and _not_ the
        // adder.
        assert_eq!(dcgka.my_id, processor_id);
        assert_ne!(adder_id, processor_id);
        assert_ne!(added_id, processor_id);

        // Control messages
        // ~~~~~~~~~~~~~~~~

        // Processor broadcasts an "add-ack" control message to everyone.
        let Some(ControlMessage::AddAck {
            ack_sender,
            ack_seq,
        }) = output.control_message
        else {
            panic!("expected \"add-ack\" control message");
        };

        // "Processor" acknowledges the "add" message of the "adder".
        assert_eq!(ack_sender, adder_id);
        assert_eq!(ack_seq, seq);

        // Direct messages
        // ~~~~~~~~~~~~~~~

        // "Processor" forwards a direct message to "added". It is required so the "added" member
        // can decrypt subsequent messages of the "processing" member.
        assert_eq!(output.direct_messages.len(), 1);
        assert_eq!(output.direct_messages.first().unwrap().recipient, added_id);
        assert_eq!(
            output.direct_messages.first().unwrap().message_type(),
            DirectMessageType::Forward
        );

        // Update Secrets
        // ~~~~~~~~~~~~~~

        // Remember "processor's" update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, processor_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Remember "adder's" update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, adder_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // "Processor" should be aware now of "adder's" update secret.
        self.assert_update_secrets(processor_id, adder_id);

        // Key Material
        // ~~~~~~~~~~~~

        // Seed was never used and should be none.
        assert!(dcgka.next_seed.is_none());

        // Member secret for the "added" was established.
        assert!(
            dcgka
                .member_secrets
                .contains_key(&(adder_id, seq, added_id))
        );

        // No ratchet secret exists yet for "added".
        assert!(!dcgka.ratchet.contains_key(&added_id));
    }

    /// Expected local state after processing an "add-ack" control message.
    pub fn assert_process_add_ack(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>,
        processor_id: MemberId, // "Processor" of the "add-ack" control message
        add_acker_id: MemberId, // Sender of the "add-ack" control message
    ) {
        // The given state is from the "processor" and not the sender of the "add-ack" message.
        assert_eq!(dcgka.my_id, processor_id);
        assert_ne!(add_acker_id, processor_id);

        // Control messages
        // ~~~~~~~~~~~~~~~~

        // No control messages.
        assert!(output.control_message.is_none());

        // Direct messages
        // ~~~~~~~~~~~~~~~

        // No direct messages.
        assert!(output.direct_messages.is_empty());

        // Update Secrets
        // ~~~~~~~~~~~~~~

        // No new update secret for "processor's" own message ratchet.
        assert!(output.me_update_secret.is_none());

        // "Processor" establishes the update secret for "add-acking" member's message ratchet.
        assert!(output.sender_update_secret.is_some());

        // Remember "add-ackers's" update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, add_acker_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // "Processor" should be aware now of "add-acker's" update secret.
        self.assert_update_secrets(processor_id, add_acker_id);

        // Key Material
        // ~~~~~~~~~~~~

        // Seed was never used and should be none.
        assert!(dcgka.next_seed.is_none());

        // Ratchet secret exists for "add-ack".
        assert!(dcgka.ratchet.contains_key(&add_acker_id));
    }

    /// Expected local state after removing a member from the group.
    pub fn assert_remove(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &OperationOutput<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>,
        remover_id: MemberId,          // Author of the "remove" control message
        removed_id: MemberId,          // Member which gets "removed"
        expected_members: &[MemberId], // List of expected members after removal
        seq: MessageId,                // Id of "remove" control message
    ) {
        // This is a local group operation, so we expect the state to be from the "remover".
        assert_eq!(dcgka.my_id, remover_id);
        assert_ne!(removed_id, remover_id);

        // Control messages
        // ~~~~~~~~~~~~~~~~

        // "Remover" broadcasts a "remove" control message to everyone.
        let ControlMessage::Remove { removed } = output.control_message else {
            panic!("expected \"remove\" control message");
        };
        assert_eq!(removed, removed_id);

        // Direct messages
        // ~~~~~~~~~~~~~~~

        // "Remover" sends a direct 2SM message to each other member of the group who is left.
        assert_eq!(
            output.direct_messages.len(),
            members_without(expected_members, &[remover_id, removed_id]).len(),
        );
        for (index, expected_member) in members_without(expected_members, &[remover_id, removed_id])
            .iter()
            .enumerate()
        {
            assert_eq!(
                output.direct_messages.get(index).unwrap().message_type(),
                DirectMessageType::TwoParty,
                "remove operation should yield a 2SM direct message"
            );
            assert_eq!(
                output.direct_messages.get(index).unwrap().recipient,
                *expected_member,
                "direct message should address expected member",
            );
        }

        // Update Secrets
        // ~~~~~~~~~~~~~~

        // "Remover" establishes a new update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // Remember "remover's" update secret for later assertions.
        self.update_secrets.insert(
            (remover_id, remover_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Key Material
        // ~~~~~~~~~~~~

        // Seed secret has been dropped after removal (FS).
        assert!(dcgka.next_seed.is_none());

        // "Remover" established member secrets for all expected members of the group.
        assert_eq!(
            dcgka.member_secrets.len(),
            members_without(expected_members, &[remover_id, removed_id]).len()
        );
        for member_id in members_without(expected_members, &[remover_id, removed_id]) {
            assert!(
                dcgka
                    .member_secrets
                    .contains_key(&(remover_id, seq, member_id))
            );
        }
    }

    /// Expected local state after processing a "remove" control message.
    pub fn assert_process_remove(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>,
        processor_id: MemberId,
        remover_id: MemberId,
        seq: MessageId,
    ) {
        // We're looking at the state of the processor.
        assert_eq!(dcgka.my_id, processor_id);
        assert_ne!(remover_id, processor_id);

        // Control messages
        // ~~~~~~~~~~~~~~~~

        // "Processor" of "remove" message broadcasts an "ack" control message to everyone.
        let Some(ControlMessage::Ack {
            ack_sender,
            ack_seq,
        }) = output.control_message
        else {
            panic!("expected \"ack\" control message");
        };

        // "Processor" acknowledges the "remove" message of the "remover".
        assert_eq!(ack_sender, remover_id);
        assert_eq!(ack_seq, seq);

        // Direct messages
        // ~~~~~~~~~~~~~~~

        // No direct messages.
        assert!(output.direct_messages.is_empty());

        // Update Secrets
        // ~~~~~~~~~~~~~~

        // "Processor" establishes the update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // "Processor" establishes the update secret for "remover's" message ratchet.
        assert!(output.sender_update_secret.is_some());

        // Remember "processor's" update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, processor_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Remember "remover's" update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, remover_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // "Processor" should be aware now of "remover's" update secret.
        self.assert_update_secrets(processor_id, remover_id);

        // Key Material
        // ~~~~~~~~~~~~

        // Seed was never used and should be none.
        assert!(dcgka.next_seed.is_none());

        // Update secret of "remover" was dropped after use. (FS)
        assert!(
            !dcgka
                .member_secrets
                .contains_key(&(remover_id, seq, processor_id))
        );

        // Outer-Ratchet holds secrets for both "processor" and "remover".
        assert!(dcgka.ratchet.contains_key(&processor_id));
        assert!(dcgka.ratchet.contains_key(&remover_id));
    }

    /// Expected local state after a member updated the group.
    pub fn assert_update(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &OperationOutput<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>,
        updater_id: MemberId,          // Member updating the group
        expected_members: &[MemberId], // List of expected members during update
        seq: MessageId,                // Id of the "update" control message
    ) {
        // This is a local group operation, so we expect the state to be from the "updater".
        assert_eq!(dcgka.my_id, updater_id);

        // Control messages
        // ~~~~~~~~~~~~~~~~

        // "Updater" broadcasts an "update" control message to everyone.
        let ControlMessage::Update = output.control_message else {
            panic!("expected \"update\" control message");
        };

        // "Updater" sends a direct 2SM message to each other member of the group.
        assert_eq!(output.direct_messages.len(), expected_members.len() - 1);
        for (index, expected_member) in members_without(expected_members, &[updater_id])
            .iter()
            .enumerate()
        {
            assert_eq!(
                output.direct_messages.get(index).unwrap().message_type(),
                DirectMessageType::TwoParty,
            );
            assert_eq!(
                output.direct_messages.get(index).unwrap().recipient,
                *expected_member,
            );
        }

        // Update Secrets
        // ~~~~~~~~~~~~~~

        // "Updater" establishes a new update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // Remember "updater's" update secret for later assertions.
        self.update_secrets.insert(
            (updater_id, updater_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Key Material
        // ~~~~~~~~~~~~

        // Seed secret has been dropped after update (FS).
        assert!(dcgka.next_seed.is_none());

        // "Updater" established member secrets for all expected members of the group.
        assert_eq!(
            dcgka.member_secrets.len(),
            members_without(expected_members, &[updater_id]).len()
        );
        for member_id in members_without(expected_members, &[updater_id]) {
            assert!(
                dcgka
                    .member_secrets
                    .contains_key(&(updater_id, seq, member_id))
            );
        }

        // Outer-Ratchet contains secret from "updater".
        assert!(dcgka.ratchet.contains_key(&updater_id));
    }

    /// Expected state after processing an "update" control message.
    pub fn assert_process_update(
        &mut self,
        dcgka: &TestDcgkaState,
        output: &ProcessOutput<MemberId, MessageId, AckedTestDgm<MemberId, MessageId>>,
        processor_id: MemberId, // Member processing the "update" control message
        updater_id: MemberId,   // Member who updated the group
        seq: MessageId,         // Id of the "update" control message
    ) {
        // We're looking at the state of the processor.
        assert_eq!(dcgka.my_id, processor_id);
        assert_ne!(updater_id, processor_id);

        // Control messages
        // ~~~~~~~~~~~~~~~~

        // Processor of "update" message broadcasts an "ack" control message to everyone.
        let Some(ControlMessage::Ack {
            ack_sender,
            ack_seq,
        }) = output.control_message
        else {
            panic!("expected \"ack\" control message");
        };

        // "Processor" acknowledges the "update" message of the "updater".
        assert_eq!(ack_sender, updater_id);
        assert_eq!(ack_seq, seq);

        // Direct messages
        // ~~~~~~~~~~~~~~~

        // No direct messages.
        assert!(output.direct_messages.is_empty());

        // Update Secrets
        // ~~~~~~~~~~~~~~

        // "Processor" establishes the update secret for their own message ratchet.
        assert!(output.me_update_secret.is_some());

        // "Processor" establishes the update secret for "remover's" message ratchet.
        assert!(output.sender_update_secret.is_some());

        // Remember "processor's" update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, processor_id),
            output.me_update_secret.as_ref().unwrap().clone(),
        );

        // Remember "updater's" update secret for later assertions.
        self.update_secrets.insert(
            (processor_id, updater_id),
            output.sender_update_secret.as_ref().unwrap().clone(),
        );

        // "Processor" should be aware now of "updater's" update secret.
        self.assert_update_secrets(processor_id, updater_id);

        // Key Material
        // ~~~~~~~~~~~~

        // Seed was never used and should be none.
        assert!(dcgka.next_seed.is_none());

        // Update secret of "updater" was dropped after use. (FS)
        assert!(
            !dcgka
                .member_secrets
                .contains_key(&(updater_id, seq, processor_id))
        );

        // Outer-Ratchet holds secrets for both "processor" and "updater".
        assert!(dcgka.ratchet.contains_key(&processor_id));
        assert!(dcgka.ratchet.contains_key(&updater_id));
    }

    /// Compare if member learned about the update secret from another member.
    fn assert_update_secrets(&self, from: MemberId, to: MemberId) {
        assert_eq!(
            self.update_secrets.get(&(from, to)).unwrap().as_bytes(),
            self.update_secrets.get(&(to, to)).unwrap().as_bytes(),
        );
    }
}
