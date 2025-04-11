// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::marker::PhantomData;

use thiserror::Error;

use crate::crypto::{Rng, Secret};
use crate::key_bundle::OneTimeKeyBundle;
use crate::message_scheme::dcgka::{ControlMessage, Dcgka, DcgkaError, DcgkaState, DirectMessage};
use crate::message_scheme::ratchet::{DecryptionRatchetState, RatchetSecret, RatchetSecretState};
use crate::traits::{
    AckedGroupMembership, ForwardSecureOrdering, IdentityHandle, IdentityManager, IdentityRegistry,
    MessageInfo, OperationId, PreKeyManager, PreKeyRegistry,
};

pub struct MessageGroup<ID, OP, PKI, DGM, KMG, ORD> {
    _marker: PhantomData<(ID, OP, PKI, DGM, KMG, ORD)>,
}

pub struct MessageGroupState<ID, OP, PKI, DGM, KMG, ORD>
where
    ID: IdentityHandle,
    OP: OperationId,
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: IdentityManager<KMG::State> + PreKeyManager,
    ORD: ForwardSecureOrdering<ID, OP>,
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
    ORD: ForwardSecureOrdering<ID, OP>,
{
    pub fn init(
        my_id: ID,
        my_keys: KMG::State,
        pki: PKI::State,
        dgm: DGM::State,
        orderer: ORD::State,
    ) -> MessageGroupState<ID, OP, PKI, DGM, KMG, ORD> {
        MessageGroupState {
            my_id,
            dcgka: Dcgka::init(my_id, my_keys, pki, dgm),
            orderer,
            ratchet: None,
            decryption_ratchet: HashMap::new(),
        }
    }

    pub fn create(
        mut y: MessageGroupState<ID, OP, PKI, DGM, KMG, ORD>,
        initial_members: Vec<ID>,
        rng: &Rng,
    ) -> MessageGroupResult<ORD::Message, ID, OP, PKI, DGM, KMG, ORD> {
        // Make sure the group state is not established yet.
        assert!(y.ratchet.is_none()); // TODO: Return an error here instead.

        // Create new group with initial members.
        let (y_dcgka_i, pre) = Dcgka::create(y.dcgka, initial_members, rng)?;
        y.dcgka = y_dcgka_i;

        // Determine parameters for "create" control message.
        let (y_orderer_i, message) = ORD::next_control_message(y.orderer, &pre.control_message)
            .map_err(|err| MessageGroupError::Orderer(err))?;
        y.orderer = y_orderer_i;

        // Process "create" control message locally.
        let (y_dcgka_ii, output) = Dcgka::process_local(y.dcgka, message.id(), pre, rng)?;
        y.dcgka = y_dcgka_ii;

        // Establish our own encryption ratchet for application messages.
        y.ratchet = Some(RatchetSecret::init(
            output
                .me_update_secret
                .expect("'create' operation always yields an update secret for us")
                .into(),
        ));

        Ok((y, message))
    }

    pub fn receive(
        y: MessageGroupState<ID, OP, PKI, DGM, KMG, ORD>,
        sender: ID,
        seq: OP,
        dependencies: &[OP],
        control_message: ControlMessage<ID, OP>,
        direct_messages: Vec<DirectMessage<ID, OP, DGM>>,
    ) -> MessageGroupResult<(), ID, OP, PKI, DGM, KMG, ORD> {
        todo!()
    }

    pub fn send(
        y: MessageGroupState<ID, OP, PKI, DGM, KMG, ORD>,
        plaintext: &[u8],
    ) -> MessageGroupResult<(), ID, OP, PKI, DGM, KMG, ORD> {
        todo!()
    }

    pub fn add() {
        todo!()
    }

    pub fn remove() {
        todo!()
    }

    pub fn update() {
        todo!()
    }
}

pub type MessageGroupResult<T, ID, OP, PKI, DGM, KMG, ORD> = Result<
    (MessageGroupState<ID, OP, PKI, DGM, KMG, ORD>, T),
    MessageGroupError<ID, OP, PKI, DGM, KMG, ORD>,
>;

#[derive(Debug, Error)]
pub enum MessageGroupError<ID, OP, PKI, DGM, KMG, ORD>
where
    PKI: IdentityRegistry<ID, PKI::State> + PreKeyRegistry<ID, OneTimeKeyBundle>,
    DGM: AckedGroupMembership<ID, OP>,
    KMG: PreKeyManager,
    ORD: ForwardSecureOrdering<ID, OP>,
{
    #[error(transparent)]
    Dcgka(#[from] DcgkaError<ID, OP, PKI, DGM, KMG>),

    #[error(transparent)]
    Orderer(ORD::Error),
}
