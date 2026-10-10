// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::traits::Provenance;
use p2panda_core::{Hash, Topic, VerifyingKey};
use p2panda_spaces::GroupId;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use p2panda_stream::hooks::ProcessorHook;
use p2panda_stream::spaces::SpacesProcessorArgs;
use tracing::{debug, warn};

use crate::processor::ProcessorStatus;
use crate::spaces::group_log_id;
use crate::spaces::types::SpacesArgs;
use crate::streams::Event;

/// Hook which takes care of associating group logs with group or space topics.
///
/// These associations need to happen so that the correct logs are replicated with other peers
/// during sync.
pub struct GroupsHook {
    topic: Topic,
    store: SqliteStore,
}

impl GroupsHook {
    #[allow(unused)]
    pub fn new(topic: Topic, store: SqliteStore) -> Self {
        Self { topic, store }
    }
}

impl ProcessorHook<Event> for GroupsHook {
    async fn on_input(&self, input: &Event) {
        let author = input.operation.author();

        let ProcessorStatus::Completed(_) = &input.spaces else {
            return;
        };

        let args = match &input.spaces_args {
            SpacesProcessorArgs::Process { msg } => &msg.args,
            SpacesProcessorArgs::AlreadyProcessed { msg, .. } => &msg.args,
            SpacesProcessorArgs::Ignore => return,
        };

        let SpacesArgs::Group { group_id, .. } = args else {
            return;
        };

        // For every group message received in the hook make a topic -> log association. This
        // works based on the assumption that group operations are processed in every space and
        // group that needs them.
        let _ = associate_group_log(&self.store, self.topic, author, *group_id).await;
    }
}

async fn associate_group_log(
    store: &SqliteStore,
    topic: Topic,
    author: VerifyingKey,
    group_id: GroupId,
) -> Result<(), SqliteError> {
    let log_id = group_log_id(group_id);
    let result = tx!(store, store.associate_tx(&topic, &author, &log_id).await);

    if let Err(err) = result {
        warn!("error making log association in groups hook: {err}");
        return Ok(());
    };

    debug!(
        topic=%topic,
        log_id=%Hash::from_bytes(*log_id.as_bytes()),
        author=%author,
        "group log associated with topic"
    );

    Ok(())
}
