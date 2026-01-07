// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::time::Instant;

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorCell, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use tracing::{trace, warn};

use crate::supervisor::config::RestartStrategy;
use crate::supervisor::traits::ChildActor;

pub enum ToSupervisorActor {
    StartChildActor(Box<dyn ChildActor + 'static>, RpcReplyPort<()>),
}

struct ChildActorState {
    child: Box<dyn ChildActor + 'static>,
    #[allow(unused)]
    first_started: Instant,
    last_restarted: Option<Instant>,
    restarts: usize,
    failures: usize,
}

impl ChildActorState {
    pub fn new(child: Box<dyn ChildActor + 'static>) -> Self {
        Self {
            child,
            first_started: Instant::now(),
            last_restarted: None,
            restarts: 0,
            failures: 0,
        }
    }
}

pub struct SupervisorActorState {
    restart_strategy: RestartStrategy,
    children: HashMap<ActorCell, ChildActorState>,
    thread_pool: ThreadLocalActorSpawner,
}

pub type SupervisorActorArgs = (RestartStrategy, ThreadLocalActorSpawner);

#[derive(Default)]
pub struct SupervisorActor;

impl ThreadLocalActor for SupervisorActor {
    type Msg = ToSupervisorActor;

    type State = SupervisorActorState;

    type Arguments = SupervisorActorArgs;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (restart_strategy, thread_pool) = args;

        Ok(SupervisorActorState {
            restart_strategy,
            children: HashMap::new(),
            thread_pool,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToSupervisorActor::StartChildActor(child, reply) => {
                let actor_cell = child
                    .on_start(myself.into(), state.thread_pool.clone())
                    .await?;

                state
                    .children
                    .insert(actor_cell, ChildActorState::new(child));

                let _ = reply.send(());
            }
        }

        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorStarted(actor_cell) => {
                trace!(actor_id = %actor_cell.get_id(), "child actor started");
            }
            SupervisionEvent::ActorTerminated(actor_cell, _, _) => {
                trace!(actor_id = %actor_cell.get_id(), "child actor terminated");
            }
            SupervisionEvent::ActorFailed(actor_cell, err) => {
                warn!(actor_id = %actor_cell.get_id(), "child actor failed: {err:?}");

                match state.restart_strategy {
                    RestartStrategy::OneForOne => {
                        if let Some(mut child_state) = state.children.remove(&actor_cell) {
                            child_state.restarts += 1;
                            child_state.failures += 1;
                            child_state.last_restarted = Some(Instant::now());
                            let next_actor_cell = child_state
                                .child
                                .on_start(myself.clone().into(), state.thread_pool.clone())
                                .await?;
                            state.children.insert(next_actor_cell, child_state);
                        }
                    }
                    RestartStrategy::OneForAll => {
                        let mut next_children = HashMap::new();

                        for (child_actor, mut child_state) in state.children.drain() {
                            // Terminate this actor.
                            child_actor.stop(None);

                            // .. and restart it directly again.
                            if actor_cell == child_actor {
                                child_state.failures += 1;
                            }
                            child_state.restarts += 1;
                            child_state.last_restarted = Some(Instant::now());

                            let next_actor_cell = child_state
                                .child
                                .on_start(myself.clone().into(), state.thread_pool.clone())
                                .await?;
                            next_children.insert(next_actor_cell, child_state);
                        }

                        state.children = next_children;
                    }
                }
            }
            _ => (),
        }

        Ok(())
    }
}
