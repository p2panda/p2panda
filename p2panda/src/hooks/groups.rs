// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::traits::Provenance;
use p2panda_core::{Hash, Topic, VerifyingKey};
use p2panda_spaces::SpaceId;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use p2panda_stream::hooks::ProcessorHook;
use p2panda_stream::spaces::SpacesProcessorArgs;
use tracing::{debug, warn};

use crate::processor::ProcessorStatus;
use crate::spaces::group_log_id;
use crate::spaces::types::SpacesArgs;
use crate::streams::Event;

/// Hook which takes care of associating all the group logs a space needs with it's topic.
///
/// These associations need to happen so that the correct logs are replicated with other peers
/// during sync. There are two reasons a groups log needs to be associated with a space topic.
///
/// 1. The group is/becomes a member of the space
/// 2. To share a group with other space subscribers before it is a member
///
/// Point 2. is needed so that a space admin can discover and add the group to the space
/// themselves. This "group discovery" pattern can also be solved by any number of invite flows,
/// including side-channel sharing.
///
/// For this hook to do it's job operations for all groups which fall into the two categories
/// above must be processed through the space pipeline.
///
/// These are the scenarios where the group log association must be updated by group operations
/// being sent to the space processing pipeline.
///
/// 1) when a space is created (the group for the space itself)
/// 2) when a space is created and any initial members are groups
/// 3) when a group is added to a space
/// 4) when a group is added to an existing space member group
/// 5) when the membership of any space member group changes
/// 6) (optional) when a space subscriber wants to share a group so that it can then be added to
///    the space
///
/// All of these actions can be performed locally by ourselves, or remotely by another admin
/// member. In both cases, the required group operations must be sent through the pipeline. Point
/// 4) & 5) can happen concurrently to the space being created, or via the Group API, detecting
/// and repairing the out-of-sync space (including sending required group operations into the
/// pipeline) is the responsibility of the repair task.
pub struct GroupsHook {
    space_id: SpaceId,
    store: SqliteStore,
}

impl GroupsHook {
    pub fn new(space_id: SpaceId, store: SqliteStore) -> Self {
        Self { space_id, store }
    }
}

impl ProcessorHook<Event> for GroupsHook {
    async fn on_input(&self, input: &Event) {
        let author = input.operation.author();

        let ProcessorStatus::Completed(_) = input.spaces else {
            return;
        };

        let args = match &input.spaces_args {
            SpacesProcessorArgs::Process { msg } => &msg.args,
            SpacesProcessorArgs::AlreadyProcessed { msg, .. } => &msg.args,
            SpacesProcessorArgs::Ignore => return,
        };

        if let Err(err) = associate_group_log(&self.store, self.space_id, author, args).await {
            warn!("error making log association in groups hook: {err}");
        }
    }
}

async fn associate_group_log(
    store: &SqliteStore,
    space_id: SpaceId,
    author: VerifyingKey,
    args: &SpacesArgs,
) -> Result<(), SqliteError> {
    let SpacesArgs::Group { group_id, .. } = args else {
        return Ok(());
    };

    // For every create group message received in the hook make a topic -> log association. This
    // works based on the assumption that group operations are replicated in every space that
    // needs them. This includes the space's own group operations.
    let log_id = group_log_id(*group_id);
    tx!(
        store,
        store
            .associate(&Topic::from(space_id), &author, &log_id)
            .await?
    );

    debug!(
        topic=%space_id,
        log_id=%Hash::from_bytes(*log_id.as_bytes()),
        author=%author,
        "group log associated with space topic"
    );

    Ok(())
}
