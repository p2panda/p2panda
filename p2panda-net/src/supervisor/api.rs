// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::supervisor::ChildActorFut;
use crate::supervisor::actor::{SupervisorActor, SupervisorActorArgs, ToSupervisorActor};
use crate::supervisor::builder::Builder;
use crate::supervisor::traits::ChildActor;

#[derive(Clone)]
pub struct Supervisor {
    args: SupervisorActorArgs,
    inner: Arc<RwLock<Inner>>,
}

struct Inner {
    actor_ref: Option<ActorRef<ToSupervisorActor>>,
}

impl Supervisor {
    pub(crate) fn new(
        actor_ref: Option<ActorRef<ToSupervisorActor>>,
        args: SupervisorActorArgs,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
            args,
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
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToSupervisorActor::StartChildActor,
            Box::new(child)
        )
        .map_err(Box::new)?;
        Ok(())
    }

    pub(crate) fn thread_pool(&self) -> ThreadLocalActorSpawner {
        self.args.1.clone()
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(actor_ref) = self.actor_ref.take() {
            actor_ref.stop(None);
        }
    }
}

impl ChildActor for Supervisor {
    fn on_start(
        &self,
        supervisor: ractor::ActorCell,
        thread_pool: ThreadLocalActorSpawner,
    ) -> ChildActorFut<'_> {
        Box::pin(async move {
            // Spawn our actor as a child of the supervisor.
            let (actor_ref, _) =
                SupervisorActor::spawn_linked(None, self.args.clone(), supervisor, thread_pool)
                    .await?;

            // Update the reference to inner actor, we need this to send messages to it.
            let mut inner = self.inner.write().await;
            inner.actor_ref.replace(actor_ref.clone());

            Ok(actor_ref.into())
        })
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
