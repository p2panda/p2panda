// SPDX-License-Identifier: MIT OR Apache-2.0

//! An iroh-specific router actor for registering ALPNs.

use std::time::Duration;

use iroh::protocol::Router as IrohRouter;
use iroh::Endpoint as IrohEndpoint;
use iroh_gossip::ALPN as GOSSIP_ALPN;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message};

use crate::actors::gossip::ToGossip;

#[derive(Clone)]
pub struct RouterConfig {
    pub endpoint: IrohEndpoint,
    pub gossip: Option<ActorRef<ToGossip>>,
}

// TODO: Eventually we want to support dynamically adding and removing ALPNs.
pub enum ToRouter {}

impl Message for ToRouter {}

pub struct RouterState {
    router: IrohRouter,
}

pub struct Router;

impl Actor for Router {
    type State = RouterState;
    type Msg = ToRouter;
    type Arguments = RouterConfig;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        config: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let mut router_builder = IrohRouter::builder(config.endpoint);

        // Optionally configure the router to accept gossip connections.
        if let Some(gossip_actor) = config.gossip {
            let gossip = gossip_actor
                .call(ToGossip::Handle, Some(Duration::from_millis(500)))
                .await
                // Panics in `pre_start` are caught and wrapped in a `SpawnErr::StartupFailed`
                // error.
                .expect("failed to send message to gossip actor")
                .expect("failed to receive gossip handle from gossip actor");

            router_builder = router_builder.accept(GOSSIP_ALPN, gossip);
        }

        let router = router_builder.spawn();

        let state = RouterState { router };

        Ok(state)
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
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Shutting down the router will call `close()` on the associated iroh endpoint.
        state.router.shutdown().await?;

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
