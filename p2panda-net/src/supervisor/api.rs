// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::{ActorRef, call};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::supervisor::actor::ToSupervisorActor;
use crate::supervisor::builder::Builder;
use crate::supervisor::traits::ChildActor;

#[derive(Clone)]
pub struct Supervisor {
    inner: Arc<RwLock<Inner>>,
}

struct Inner {
    actor_ref: ActorRef<ToSupervisorActor>,
}

impl Supervisor {
    pub(crate) fn new(actor_ref: ActorRef<ToSupervisorActor>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
        }
    }

    pub fn builder() -> Builder {
        Builder::new()
    }

    pub(crate) async fn start_child_actor<C>(&self, child: C) -> Result<(), SupervisorError>
    where
        C: ChildActor + 'static,
    {
        let inner = self.inner.read().await;
        call!(
            inner.actor_ref,
            ToSupervisorActor::StartChildActor,
            Box::new(child)
        )
        .map_err(Box::new)?;
        Ok(())
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.actor_ref.stop(None);
    }
}

#[derive(Debug, Error)]
pub enum SupervisorError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToSupervisorActor>>),
}
