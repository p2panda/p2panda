// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::ActorCell;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

use crate::iroh_endpoint::actors::IrohEndpoint;
use crate::iroh_endpoint::{Builder, Endpoint, EndpointError};
use crate::supervisor::{ChildActor, ChildActorFut, Supervisor};

impl Builder {
    pub async fn spawn_linked(self, supervisor: &Supervisor) -> Result<Endpoint, EndpointError> {
        let endpoint = Endpoint::new(None, self.build_args());
        supervisor.start_child_actor(endpoint.clone()).await?;
        Ok(endpoint)
    }
}

impl ChildActor for Endpoint {
    fn on_start(
        &self,
        supervisor: ActorCell,
        thread_pool: ThreadLocalActorSpawner,
    ) -> ChildActorFut<'_> {
        Box::pin(async move {
            // Spawn our actor as a child of the supervisor.
            let (actor_ref, _) =
                IrohEndpoint::spawn_linked(None, self.args.clone(), supervisor, thread_pool)
                    .await?;

            // Update the reference to our actor, we need this to send messages to it.
            let mut inner = self.inner.write().await;
            inner.actor_ref.replace(actor_ref.clone());

            Ok(actor_ref.into())
        })
    }
}
