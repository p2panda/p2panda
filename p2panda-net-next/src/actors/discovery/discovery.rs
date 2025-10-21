// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::protocol::ProtocolHandler;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent, cast, registry};

use crate::actors::endpoint::router::{ROUTER, ToRouter};

pub const DISCOVERY: &str = "discovery";

pub const DISCOVERY_PROTOCOL_ID: &[u8] = b"p2panda/discovery/v1";

pub enum ToDiscovery {}

impl Message for ToDiscovery {}

pub struct DiscoveryState {}

pub struct Discovery;

impl Actor for Discovery {
    type State = DiscoveryState;

    type Msg = ToDiscovery;

    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(DiscoveryState {})
    }

    async fn post_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        _message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        _message: SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }
}
