// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};

use crate::supervisor::traits::ChildActor;

pub enum ToSupervisorActor {
    StartChild(Box<dyn ChildActor + 'static>),
}

#[derive(Default)]
pub struct SupervisorActor {}

impl ThreadLocalActor for SupervisorActor {
    type Msg = ToSupervisorActor;

    type State = ();

    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(())
    }
}
