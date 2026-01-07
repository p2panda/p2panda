// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

use crate::supervisor::actor::SupervisorActor;
use crate::supervisor::{RestartStrategy, Supervisor, SupervisorError};

pub struct Builder {
    strategy: RestartStrategy,
}

impl Builder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            strategy: RestartStrategy::default(),
        }
    }

    pub fn strategy(mut self, strategy: RestartStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub async fn spawn(self) -> Result<Supervisor, SupervisorError> {
        let thread_pool = ThreadLocalActorSpawner::new();

        let args = (self.strategy, thread_pool.clone());
        let (actor_ref, _) = SupervisorActor::spawn(None, args.clone(), thread_pool).await?;

        Ok(Supervisor::new(Some(actor_ref), args))
    }

    pub async fn spawn_linked(self, parent: &Supervisor) -> Result<Supervisor, SupervisorError> {
        let args = (self.strategy, parent.thread_pool());
        let supervisor = Supervisor::new(None, args);

        parent.start_child_actor(supervisor.clone()).await?;

        Ok(supervisor)
    }
}
