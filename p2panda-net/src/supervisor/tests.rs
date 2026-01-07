// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use ractor::concurrency::Duration;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorRef, RpcReplyPort, call};
use tokio::sync::RwLock;

use crate::supervisor::{ChildActor, ChildActorFut, RestartStrategy, Supervisor};
use crate::test_utils::setup_logging;

#[derive(Clone)]
struct TestApi {
    actor_ref: Arc<RwLock<Option<ActorRef<ToTestActor>>>>,
}

impl TestApi {
    #[cfg(feature = "supervisor")]
    pub async fn spawn_linked(supervisor: &Supervisor) -> Self {
        let actor = Self {
            actor_ref: Arc::new(RwLock::new(None)),
        };

        supervisor.start_child_actor(actor.clone()).await.unwrap();

        actor
    }

    pub async fn echo(&self, value: u64) -> u64 {
        let inner = self.actor_ref.read().await;
        let response = call!(inner.as_ref().unwrap(), ToTestActor::Echo, value).unwrap();
        response
    }

    pub async fn panic(&self) {
        let inner = self.actor_ref.read().await;
        inner
            .as_ref()
            .unwrap()
            .send_message(ToTestActor::Panic)
            .unwrap();
    }
}

impl ChildActor for TestApi {
    fn on_start(
        &self,
        supervisor: ractor::ActorCell,
        thread_pool: ractor::thread_local::ThreadLocalActorSpawner,
    ) -> ChildActorFut<'_> {
        Box::pin(async move {
            let args = ();

            // Spawn our actor as a child of the supervisor.
            let (actor_ref, _) =
                TestActor::spawn_linked(None, args, supervisor, thread_pool).await?;

            // Update the reference to our actor, we need this to send messages to it.
            let mut inner = self.actor_ref.write().await;
            inner.replace(actor_ref.clone());

            Ok(actor_ref.into())
        })
    }
}

#[derive(Default)]
struct TestActor;

enum ToTestActor {
    Echo(u64, RpcReplyPort<u64>),
    Panic,
}

impl ThreadLocalActor for TestActor {
    type Msg = ToTestActor;

    type State = ();

    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ractor::ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ractor::ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ractor::ActorProcessingErr> {
        match message {
            ToTestActor::Echo(value, reply) => {
                let _ = reply.send(value);
            }
            ToTestActor::Panic => {
                panic!("aaaaah!");
            }
        }

        Ok(())
    }
}

#[tokio::test]
async fn restart_after_failure() {
    setup_logging();

    let supervisor = Supervisor::builder()
        .strategy(RestartStrategy::OneForOne)
        .spawn()
        .await
        .unwrap();

    let test = TestApi::spawn_linked(&supervisor).await;

    // Actor works as expected after launching.
    assert_eq!(test.echo(15).await, 15);

    // Make it crash. The supervisor should restart it.
    test.panic().await;

    // Wait a little to allow restarting.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Actor continues to work as expected.
    assert_eq!(test.echo(27).await, 27);
}

#[tokio::test]
async fn nested_supervisors() {
    setup_logging();

    let root_supervisor = Supervisor::builder().spawn().await.unwrap();

    let child_supervisor = Supervisor::builder()
        .spawn_linked(&root_supervisor)
        .await
        .unwrap();

    let test = TestApi::spawn_linked(&child_supervisor).await;
    assert_eq!(test.echo(15).await, 15);

    test.panic().await;

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(test.echo(27).await, 27);
}
