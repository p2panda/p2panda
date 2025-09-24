// SPDX-License-Identifier: MIT OR Apache-2.0

//! Utilities to test actors.
use ractor::actor::messages::BoxedState;
use ractor::{Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use tokio::sync::oneshot::Sender;

#[derive(Debug)]
pub(crate) enum ActorResult {
    Terminated(Option<BoxedState>, Option<String>),
    Failed(ActorProcessingErr),
}

pub(crate) struct TestSupervisorState {
    result_tx: Option<Sender<ActorResult>>,
}

/// Test supervisor actor.
///
/// The supervisor is used to catch termination or failure of an actor during testing.
/// This allows assertions to be made based on the outcome of actor activity and any returned state.
///
/// ```ignore
/// // Spawn the actor you wish to test.
/// let (example_actor, example_actor_handle) =
///     Actor::spawn(None, Example, ())
///     .await
///     .unwrap();
///
/// // Spawn the test supervisor actor.
/// let (supervisor_tx, supervisor_rx) = oneshot::channel();
/// let (supervisor_actor, supervisor_actor_handle) =
///     Actor::spawn(None, TestSupervisor, supervisor_tx)
///     .await
///     .unwrap();
///
/// // Link the actor to the test supervisor.
/// example_actor.link(supervisor_actor.into());
///
/// // Perform some work...
///
/// // Stop the actor.
/// example_actor.stop(None);
///
/// // Receive the result on the oneshot channel.
/// let example_actor_result = supervisor_rx.await;
/// ```
pub(crate) struct TestSupervisor;

impl Actor for TestSupervisor {
    type Msg = ();
    type State = TestSupervisorState;
    type Arguments = Sender<ActorResult>;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        result_tx: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(TestSupervisorState {
            result_tx: Some(result_tx),
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        _message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: ractor::SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorTerminated(_, boxed_state, reason) => {
                if let Some(result_tx) = state.result_tx.take() {
                    result_tx
                        .send(ActorResult::Terminated(boxed_state, reason))
                        .unwrap();
                }
            }
            SupervisionEvent::ActorFailed(_, err) => {
                if let Some(result_tx) = state.result_tx.take() {
                    result_tx.send(ActorResult::Failed(err)).unwrap();
                }
            }
            _ => (),
        }

        myself.stop(None);

        Ok(())
    }
}
