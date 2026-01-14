// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;

use ractor::ActorCell;
use ractor::thread_local::ThreadLocalActorSpawner;

pub type ChildActorFut<'a> =
    Pin<Box<dyn Future<Output = Result<ActorCell, ractor::SpawnErr>> + Send + 'a>>;

pub trait ChildActor
where
    Self: Send + 'static,
{
    fn on_start(
        &self,
        supervisor: ActorCell,
        thread_pool: ThreadLocalActorSpawner,
    ) -> ChildActorFut<'_>;
}
