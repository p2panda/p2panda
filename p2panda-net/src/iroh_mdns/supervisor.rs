// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::ActorCell;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

use crate::iroh_mdns::actor::MdnsActor;
use crate::iroh_mdns::{Builder, MdnsDiscovery, MdnsDiscoveryError};
use crate::supervisor::{ChildActor, ChildActorFut, Supervisor};

impl Builder {
    pub async fn spawn_linked(
        self,
        supervisor: &Supervisor,
    ) -> Result<MdnsDiscovery, MdnsDiscoveryError> {
        let mdns = MdnsDiscovery::new(None, self.build_args());
        supervisor.start_child_actor(mdns.clone()).await?;
        Ok(mdns)
    }
}

impl ChildActor for MdnsDiscovery {
    fn on_start(
        &self,
        supervisor: ActorCell,
        thread_pool: ThreadLocalActorSpawner,
    ) -> ChildActorFut<'_> {
        Box::pin(async move {
            // Spawn our actor as a child of the supervisor.
            let (actor_ref, _) =
                MdnsActor::spawn_linked(None, self.args.clone(), supervisor, thread_pool).await?;

            // Update the reference to our actor, we need this to send messages to it.
            let mut inner = self.inner.write().await;
            inner.actor_ref.replace(actor_ref.clone());

            Ok(actor_ref.into())
        })
    }
}
