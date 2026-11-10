// SPDX-License-Identifier: MIT OR Apache-2.0

//! Members log association via pipeline processor.
//!
//! Logs of members contain public key bundle material we need for initial key agreement with this
//! member. This material is required when encrypting a new group key towards someone for the first
//! time in a space. This can happen when removing a member or manually rotating it ("update").
//!
//! When creating a space or adding someone the users will make sure to collect the member's key
//! bundle beforehands. However, for other members in the space we can't assume that they'll collect
//! the key bundles.
//!
//! Concretly, this is the problem we solve with automatic members log association:
//!
//! ```text
//! Node [A] creates space S with [B], [C], [D]
//!   [A] associates their member log to S
//! Node [B] joins space S
//!   [B] associates their member log to S
//! Node [B] wants to remove [C]
//!
//! --> Node [B] needs to inform [A] & [D] about the new secret but doesn't have the member log of
//! [D] yet.
//! ```
//!
//! If Node B would already associate D's member log when they've observed the CREATE event they
//! would have had a chance to receive it from A. This processor makes sure that the association
//! takes place and we have a chance to sync key bundles in the background.
//!
//! Operations of newly associated member logs are pushed to all Node in live mode to make sure key
//! bundles travel to everyone as fast as possible.
use std::cell::RefCell;
use std::collections::VecDeque;

use p2panda_auth::Access;
use p2panda_core::traits::ShortFormat;
use p2panda_core::{Topic, VerifyingKey};
use p2panda_spaces::{MemberId, SpaceId};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use p2panda_stream::Processor;
use p2panda_stream::spaces::SpacesResult;
use thiserror::Error;
use tokio::sync::Notify;
use tokio_stream::StreamExt;
use tracing::{debug, error};

use crate::egress::{DispatchConfig, EgressHandle, EventDeliveryPolicy, EventProcessingPolicy};
use crate::processor::{Event, ProcessorStatus};
use crate::spaces::member_log_id;

/// Pipeline processor to observe spaces events and automatically associate member logs to the space
/// when someone new was added.
pub struct MembersProcessor<L, E, TP> {
    store: SqliteStore,
    egress_handle: EgressHandle,
    notify: Notify,
    #[allow(clippy::type_complexity)]
    queue: RefCell<VecDeque<(Event<L, E, TP>, MembersProcessorResult)>>,
}

impl<L, E, TP> MembersProcessor<L, E, TP> {
    pub fn new(store: SqliteStore, egress_handle: EgressHandle) -> Self {
        Self {
            store,
            egress_handle,
            notify: Notify::new(),
            queue: RefCell::new(VecDeque::new()),
        }
    }
}

impl<L, E, TP> Processor<Event<L, E, TP>> for MembersProcessor<L, E, TP> {
    type Output = (Event<L, E, TP>, MembersProcessorResult);

    type Error = (Event<L, E, TP>, MembersProcessorError);

    async fn process(&self, input: Event<L, E, TP>) -> Result<(), Self::Error> {
        match associate_members(&self.store, &self.egress_handle, &input).await {
            Ok(result) => {
                self.queue.borrow_mut().push_back((input, result));
                self.notify.notify_one();
                Ok(())
            }
            Err(error) => Err((input, MembersProcessorError(error.to_string()))),
        }
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        loop {
            if let Some(item) = self.queue.borrow_mut().pop_front() {
                return Ok(item);
            }

            self.notify.notified().await;
        }
    }
}

#[derive(Clone, Debug)]
pub enum MembersProcessorResult {
    Processed { new_members_len: usize },
    Ignored,
}

#[derive(Clone, Debug, Error)]
#[error("{0}")]
pub struct MembersProcessorError(String);

/// Associate member logs by observing spaces events.
async fn associate_members<L, E, TP>(
    store: &SqliteStore,
    egress_handle: &EgressHandle,
    input: &Event<L, E, TP>,
) -> Result<MembersProcessorResult, SqliteError> {
    let ProcessorStatus::Completed(ref result) = input.spaces else {
        return Ok(MembersProcessorResult::Ignored);
    };

    let SpacesResult::Processed {
        events: spaces_events,
    } = result
    else {
        return Ok(MembersProcessorResult::Ignored);
    };

    let mut new_members_len = 0;

    for space_event in spaces_events {
        let p2panda_spaces::Event::Spaces(space_event) = space_event else {
            continue;
        };

        let (space_id, context) = match space_event {
            p2panda_spaces::SpaceEvent::Created {
                space_id, context, ..
            } => (space_id, context),
            p2panda_spaces::SpaceEvent::Added {
                space_id, context, ..
            } => (space_id, context),
            _ => continue,
        };

        // We have to look at _current_ members instead of only the added ones since we might not
        // process all events from the beginning, especially if we've been added later to the group.
        let members = &context.members;

        match associate_members_inner(store, space_id, members).await {
            Ok(new_members) => {
                new_members_len += new_members.len();

                debug!(
                    space_id = %space_id.fmt_short(),
                    "associate {} member logs, {} new", members.len(), new_members.len()
                );

                if !new_members.is_empty() {
                    let store = store.clone();
                    let egress_handle = egress_handle.clone();
                    let topic = Topic::from(*space_id);

                    tokio::spawn(async move {
                        let result =
                            dispatch_member_logs(store, egress_handle, topic, new_members).await;

                        // Dispatch errors are not propagated as this is not critical. Nodes will
                        // need to re-sync if this failed.
                        if let Err(err) = &result {
                            error!(
                                space_id = %topic.fmt_short(),
                                "dispatching member logs failed: {err}"
                            );
                        }
                    });
                }
            }
            Err(err) => {
                error!(
                    space_id = %space_id.fmt_short(),
                    "member association failed: {err}"
                );

                return Err(err);
            }
        }
    }

    Ok(MembersProcessorResult::Processed { new_members_len })
}

async fn associate_members_inner(
    store: &SqliteStore,
    space_id: &SpaceId,
    members: &[(MemberId, Access)],
) -> Result<Vec<MemberId>, SqliteError> {
    let log_id = member_log_id();
    let mut new_members = Vec::new();

    tx!(store, {
        for (id, _) in members {
            if store.associate_tx(space_id, id, &log_id).await? {
                new_members.push(*id);
            }
        }
    });

    Ok(new_members)
}

/// Dispatch the full member log to all live nodes.
async fn dispatch_member_logs(
    store: SqliteStore,
    egress_handle: EgressHandle,
    topic: Topic,
    members: Vec<VerifyingKey>,
) -> Result<(), ReplayMemberLogsError> {
    let log_id = member_log_id();

    let config = DispatchConfig {
        delivery: EventDeliveryPolicy::Topic(topic),
        processing: EventProcessingPolicy::Disabled,
    };

    for member in members {
        // We need to dispatch the full log as we can't assume that everyone who receives it (live
        // mode nodes) will have all previous entries. Otherwise these operations will be rejected
        // as there's gaps in the log.
        //
        // TODO: Later we want to prune the log when key bundles expired, so we don't send a whole
        // log with outdated material inside.
        let mut entries = store.log_entries(&member, &log_id, None, None)?;

        while let Some(result) = entries.next().await {
            let operation = result?
                .entry
                .try_into()
                .expect("already validated extensions");

            egress_handle.dispatch_with_config(operation, &config).await;
        }
    }

    Ok(())
}

#[derive(Debug, Error)]
pub enum ReplayMemberLogsError {
    #[error(transparent)]
    Sqlite(#[from] SqliteError),
}
