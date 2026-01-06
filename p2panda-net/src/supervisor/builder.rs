// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

use crate::supervisor::actor::SupervisorActor;
use crate::supervisor::{RestartStrategy, Supervisor, SupervisorError};

pub struct Builder {
    strategy: RestartStrategy,
}

impl Builder {
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
        let (actor_ref, _) = SupervisorActor::spawn(None, args, thread_pool).await?;

        Ok(Supervisor::new(actor_ref))
    }
}
