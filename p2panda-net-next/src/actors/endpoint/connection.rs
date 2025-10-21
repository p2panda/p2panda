// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::endpoint::Connecting as IrohConnecting;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent};

pub enum ToConnection {}

impl Message for ToConnection {}

pub struct ConnectionState {}

pub struct Connection;

impl Actor for Connection {
    type State = ConnectionState;

    type Msg = ToConnection;

    type Arguments = (IrohConnecting,);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(ConnectionState {})
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
        message: Self::Msg,
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
