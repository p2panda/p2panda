// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_stream::hooks::ProcessorHook;
use p2panda_stream::spaces::SpacesProcessorArgs;
use tracing::warn;

use crate::processor::ProcessorStatus;
use crate::spaces::repair::RepairTask;
use crate::spaces::types::SpacesArgs;
use crate::streams::Event;

/// Hook for triggering the global repair task whenever a groups event is processed on any group
/// or spaces stream.
pub struct RepairHook {
    task: RepairTask,
}

impl RepairHook {
    pub fn new(task: RepairTask) -> Self {
        Self { task }
    }
}

impl ProcessorHook<Event> for RepairHook {
    async fn on_input(&self, input: &Event) {
        let ProcessorStatus::Completed(_) = &input.spaces else {
            return;
        };

        let args = match &input.spaces_args {
            SpacesProcessorArgs::Process { msg } => &msg.args,
            SpacesProcessorArgs::AlreadyProcessed { msg, .. } => &msg.args,
            SpacesProcessorArgs::Ignore => return,
        };

        let SpacesArgs::Group { .. } = args else {
            return;
        };

        if let Err(err) = self.task.sync_and_repair_all_debounced() {
            warn!("error sending to repair task: {}", err)
        };
    }
}
