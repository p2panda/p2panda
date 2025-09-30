// SPDX-License-Identifier: MIT OR Apache-2.0

//! An iroh-specific router actor for registering ALPNs.

use std::time::Duration;

use iroh::protocol::Router as IrohRouter;
use iroh::Endpoint as IrohEndpoint;
use iroh_gossip::ALPN as GOSSIP_ALPN;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message};

// TODO: Remove once used.
#[allow(dead_code)]
pub struct RouterConfig {}

pub enum ToRouter {}

impl Message for ToRouter {}

pub struct RouterState {}

pub struct Router;

impl Actor for Router {
    type State = RouterState;
    type Msg = ToRouter;
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(RouterState {})
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
}
