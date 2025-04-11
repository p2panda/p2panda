// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::marker::PhantomData;

use thiserror::Error;

use crate::crypto::Rng;
use crate::key_bundle::OneTimeKeyBundle;
use crate::message_scheme::dcgka::{
    ControlMessage, Dcgka, DcgkaError, DcgkaState, DirectMessage, OperationOutput,
};
use crate::message_scheme::ratchet::{DecryptionRatchetState, RatchetSecret, RatchetSecretState};
use crate::traits::{
    AckedGroupMembership, ForwardSecureOrdering, IdentityHandle, IdentityManager, IdentityRegistry,
    MessageInfo, OperationId, PreKeyManager, PreKeyRegistry,
};

pub struct MessageGroup<ID, OP, PKI, DGM, KMG, ORD> {
    _marker: PhantomData<(ID, OP, PKI, DGM, KMG, ORD)>,
}

pub struct GroupState<ID, OP, PKI, DGM, KMG, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    my_id: ID,
    dcgka: DcgkaState<ID, OP, PKI, DGM, KMG>,
    orderer: ORD::State,
    ratchet: Option<RatchetSecretState>,
    decryption_ratchet: HashMap<ID, DecryptionRatchetState>,
}

impl<ID, OP, PKI, DGM, KMG, ORD> MessageGroup<ID, OP, PKI, DGM, KMG, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    pub fn init(
        my_id: ID,
        my_keys: KMG::State,
        pki: PKI::State,
        dgm: DGM::State,
        orderer: ORD::State,
    ) -> GroupState<ID, OP, PKI, DGM, KMG, ORD> {
        GroupState {
            my_id,
            dcgka: Dcgka::init(my_id, my_keys, pki, dgm),
            orderer,
            ratchet: None,
            decryption_ratchet: HashMap::new(),
        }
    }

    pub fn create(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        initial_members: Vec<ID>,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        // If we have an encryption ratchet we already established a group (either by creating or
        // processing a "welcome" message in the past).
        if y.ratchet.is_some() {
            return Err(MessageGroupError::GroupAlreadyEstablished);
        }

        // Create new group with initial members.
        let (y_dcgka_i, pre) = Dcgka::create(y.dcgka, initial_members, rng)?;
        y.dcgka = y_dcgka_i;

        Ok(Self::process_local(y, pre, rng)?)
    }

    pub fn add(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        added: ID,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if y.ratchet.is_none() {
            return Err(MessageGroupError::GroupNotYetEstablished);
        }

        if y.my_id == added {
            return Err(MessageGroupError::NotAddOurselves);
        }

        // Add a new member to the group.
        let (y_dcgka_i, pre) = Dcgka::add(y.dcgka, added, rng)?;
        y.dcgka = y_dcgka_i;

        Ok(Self::process_local(y, pre, rng)?)
    }

    pub fn remove(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        removed: ID,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if y.ratchet.is_none() {
            return Err(MessageGroupError::GroupNotYetEstablished);
        }

        // Remove a member from the group.
        let (y_dcgka_i, pre) = Dcgka::remove(y.dcgka, removed, rng)?;
        y.dcgka = y_dcgka_i;

        // TODO: Handle removing ourselves.

        Ok(Self::process_local(y, pre, rng)?)
    }

    pub fn update(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        if y.ratchet.is_none() {
            return Err(MessageGroupError::GroupNotYetEstablished);
        }

        // Update the group by generating a new seed.
        let (y_dcgka_i, pre) = Dcgka::update(y.dcgka, rng)?;
        y.dcgka = y_dcgka_i;

        Ok(Self::process_local(y, pre, rng)?)
    }

    pub fn receive(
        y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        sender: ID,
        seq: OP,
        dependencies: &[OP],
        control_message: ControlMessage<ID, OP>,
        direct_messages: Vec<DirectMessage<ID, OP, DGM>>,
    ) -> GroupResult<(), ID, OP, PKI, DGM, KMG, ORD> {
        todo!()
    }

    pub fn send(
        y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        plaintext: &[u8],
    ) -> GroupResult<(), ID, OP, PKI, DGM, KMG, ORD> {
        if y.ratchet.is_none() {
            return Err(MessageGroupError::GroupNotYetEstablished);
        }

        todo!()
    }

    fn process_local(
        mut y: GroupState<ID, OP, PKI, DGM, KMG, ORD>,
        output: OperationOutput<ID, OP, DGM>,
        rng: &Rng,
    ) -> GroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        // Determine parameters for to-be-published control message.
        let (y_orderer_i, message) =
            ORD::next_control_message(y.orderer, &output.control_message, &output.direct_messages)
                .map_err(|err| MessageGroupError::Orderer(err))?;
        y.orderer = y_orderer_i;

        // Process control message locally to update our state.
        let (y_dcgka_i, process) = Dcgka::process_local(y.dcgka, message.id(), output, rng)?;
        y.dcgka = y_dcgka_i;

        // Update own encryption ratchet for sending application messages.
        y.ratchet = Some(RatchetSecret::init(
            process
                .me_update_secret
                .expect("local operation always yields an update secret for us")
                .into(),
        ));

        Ok((y, message))
    }
}

pub type GroupResult<T, ID, OP, PKI, DGM, KMG, ORD> = Result<
    (GroupState<ID, OP, PKI, DGM, KMG, ORD>, T),
    MessageGroupError<ID, OP, PKI, DGM, KMG, ORD>,
>;

#[derive(Debug, Error)]
pub enum MessageGroupError<ID, OP, PKI, DGM, KMG, ORD>
where
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: PreKeyManager,
    ORD: ForwardSecureOrdering<ID, OP, DGM>,
{
    #[error(transparent)]
    Dcgka(#[from] DcgkaError<ID, OP, PKI, DGM, KMG>),

    #[error(transparent)]
    Orderer(ORD::Error),

    #[error("creating or joining a group is not possible, state is already established")]
    GroupAlreadyEstablished,

    #[error("state is not ready yet, group needs to be created or joined first")]
    GroupNotYetEstablished,

    #[error("can not add outselves to the group")]
    NotAddOurselves,
}
