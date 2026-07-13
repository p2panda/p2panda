// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use futures_util::stream::BoxStream;
use p2panda_core::traits::{Provenance, ShortFormat};
use p2panda_core::{Hash, Topic};
use p2panda_spaces::manager::GLOBAL_GROUPS_CONTEXT_ID;
use p2panda_spaces::{AuthGroupState, GroupId, SpaceId, SpacesStoreState};
use p2panda_store::groups::GroupsStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::spaces::SpacesStore as SpacesStoreTrait;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, Transaction};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::{self};
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::RecvError;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, trace, warn};

use crate::operation::Operation;
use crate::spaces::group_log_id;
use crate::spaces::types::{AuthCapabilities, SpacesArgs, SpacesStore};
use crate::spaces::{SpacesManagerError, types::SpacesManager};
use crate::streams::{LocalStreamFuture, StreamEvent};

const REPAIR_FREQUENCY_SECS: u64 = 1;

/// Strategy by which a space should be repaired.
///
/// When merging operations from the shared groups state into a space there are two possible
/// approaches.
///
/// ## Global
///
/// Operations for all known groups are merged into the space, even if they are not used in the
/// space yet. This results in improved discoverability (new groups are "automatically"
/// discovered) at the expense of privacy (even if a group is not added to a space it is
/// replicated on the space topic).
///
/// ## Partial
///
/// Only operations for groups added to a space (via a local action or by explicit association)
/// are merged into the space. This results in improved privacy as there is no group "leakage"
/// from the shared state into the space, however it means the initial "discovery" of a new
/// to-be-added group must be solved via another channel (side-channel, dedicated topic, etc..).
///
/// @TODO: This initial discovery mechanism is not yet implemented, it may be solved via invite
/// tokens, or manually exporting and then registering a member group. Therefore all spaces use
/// the "Global" strategy for now.
#[derive(Debug)]
pub enum RepairStrategy {
    Global,
    Partial(Vec<GroupId>),
}

/// Repairing a space is the process of merging missing auth operations from the shared groups
/// state into a space. This keeps the space membership up-to-date with concurrent changes and
/// ensures that all required auth operations are encrypted and sent to other nodes subscribed the
/// space.
///
/// There are 3 steps to this process:
///
/// 1) re-publish missing groups operations into the space topic
/// 2) create and publish space membership operations for each missing groups operation (only read
///    members can do this)
/// 3) associate missing groups logs with the space topic
///
/// All new messages will be sent into the topic stream to be processed and forwarded to other
/// peers.
pub(crate) async fn repair_space<M>(
    space_id: SpaceId,
    scope: RepairStrategy,
    manager: &SpacesManager,
    store: &SqliteStore,
    import_local_tx: &mpsc::Sender<(
        BoxStream<'static, Operation>,
        oneshot::Sender<LocalStreamFuture>,
    )>,
    to_output_tx: &mpsc::Sender<Vec<StreamEvent<M>>>,
) -> Result<bool, RepairError> {
    let spaces_store = SpacesStore::new(store.clone());

    // Collect all missing groups operations. These will be imported into the space and forwarded
    // to live-mode peers.
    let permit = store.begin().await?;

    let Some(space_y): Option<SpacesStoreState<AuthCapabilities>> =
        spaces_store.get_space_state_tx(&space_id).await?
    else {
        // This can happen if we didn't receive any space control messages yet.
        trace!(
            node_id = manager.id().fmt_short(),
            space_id = space_id.fmt_short(),
            "space not yet materialised"
        );
        store.commit(permit).await?;
        return Ok(false);
    };

    let groups_y: AuthGroupState<AuthCapabilities> = spaces_store
        .get_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID))
        .await?
        .unwrap_or_default();

    store.commit(permit).await?;

    let include = match scope {
        RepairStrategy::Global => groups_y.groups_global(),
        RepairStrategy::Partial(group_ids) => group_ids,
    };

    // Identify if this space needs repairing.
    let repair = match manager.space_repair_required(space_id, &include).await {
        Ok(ids) => ids,
        Err(err) => {
            return Err(err.into());
        }
    };

    // If not return now already.
    if !repair {
        return Ok(false);
    }

    // Collect all missing groups operations. These will be imported into the space and forwarded
    // to live-mode peers.
    let permit = store.begin().await?;

    let mut groups_operations = vec![];
    for id in groups_y.inner.toposort(&include) {
        if space_y.groups_y.inner.operations.contains_key(&id) {
            continue;
        }

        let Some(operation): Option<Operation> = store.get_operation_tx(&id).await? else {
            warn!("missing expected auth groups operation");
            continue;
        };

        // Ignore non-groups operations.
        let Some(SpacesArgs::Auth {
            group_id,
            group_action,
            ..
        }) = operation.header.extensions.spaces_args()
        else {
            warn!("expected auth groups operation");
            continue;
        };

        // If this is a create operation then associate the groups log with this space topic.
        if group_action.is_create() {
            store
                .associate(
                    &Topic::from(space_id),
                    &operation.author(),
                    &group_log_id(group_id),
                )
                .await?;
        }

        groups_operations.push(operation)
    }

    store.commit(permit).await?;

    // Attempt to repair the space. As we pass in an array containing a single space id there will
    // be only ever max one result returned.
    //
    // Forging these spaces messages will also associate any group logs with this space topic.
    //
    // @TODO: This method uses transactions internally (eg. in the Forge) and so we can't make
    // everything part of one transaction on this level yet. It isn't a source of bugs though so
    // for now this is ok.
    let (space_y, spaces_messages, events) = manager.repair_space(space_id, &include).await?;

    // If no space messages were forged during repairing then no state change occurred and we
    // don't need to persist here. This occurs when we are not a _read_ member of the space (yet).
    //
    // @TODO: Once control messages are encrypted it will not be possible for non-read members to
    // receives any control messages and so this logic can be refactored.
    if !spaces_messages.is_empty() {
        let permit = store.begin().await?;

        let space_id = space_y.space_id;
        spaces_store
            .set_space_state_tx(&space_id, &SpacesStoreState::from(space_y))
            .await?;

        store.commit(permit).await?;
    }

    // If there are no messages to send then exit here.
    if spaces_messages.is_empty() && groups_operations.is_empty() {
        return Ok(false);
    }

    // Send all resulting operations into the stream.
    let op_count = groups_operations.len() + spaces_messages.len();
    let operations = groups_operations.into_iter().chain(
        spaces_messages
            .into_iter()
            .map(|message| message.into_operation()),
    );
    let stream = Box::pin(futures_util::stream::iter(operations));
    let (ready_tx, ready_rx) = oneshot::channel::<LocalStreamFuture>();
    import_local_tx
        .send((stream, ready_tx))
        .await
        .map_err(|err| RepairError::SendToProcessor(err.to_string()))?;

    // Await processing of operations to be complete.
    ready_rx.await?;

    let events = events
        .into_iter()
        .filter_map(|event| match event {
            p2panda_spaces::Event::Space(space_event) => Some(StreamEvent::Space(space_event)),
            _ => None,
        })
        .collect();

    to_output_tx
        .send(events)
        .await
        .map_err(|_| RepairError::AppSend)?;

    debug!(
        node_id = manager.id().fmt_short(),
        space_id = space_id.fmt_short(),
        operations = op_count,
        "space repaired"
    );

    Ok(true)
}

/// Spawn the repair task which triggers work on a schedule or from being signalled from elsewhere
/// in the node API.
#[allow(clippy::type_complexity)]
pub(crate) fn spawn_repair_task<M>(
    topic: Topic,
    manager: SpacesManager,
    store: SqliteStore,
    import_local_tx: mpsc::Sender<(
        BoxStream<'static, Operation>,
        oneshot::Sender<LocalStreamFuture>,
    )>,
    to_output_tx: mpsc::Sender<Vec<StreamEvent<M>>>,
    mut repair_rx: mpsc::Receiver<(RepairStrategy, oneshot::Sender<Result<bool, RepairError>>)>,
) -> JoinHandle<()>
where
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            let args = tokio::select! {
                // We received a signal to repair the space.
                msg = repair_rx.recv() => {
                    match msg {
                        Some(args) => Some(args),
                        None => {
                            // If the repair_tx is dropped then we can exit this task.
                            return;
                        }
                    }
                }
                // Scheduled repair triggered.
                _ = sleep(Duration::from_secs(REPAIR_FREQUENCY_SECS)) => None,
            };

            match args {
                Some((scope, reply_tx)) => {
                    let result = repair_space(
                        topic.into(),
                        scope,
                        &manager,
                        &store,
                        &import_local_tx,
                        &to_output_tx,
                    )
                    .await;

                    if let Err(ref err) = result {
                        warn!("failed to repair spaces: {}", err);
                    }

                    let _ = reply_tx.send(result);
                }
                None => {
                    let result = repair_space(
                        topic.into(),
                        RepairStrategy::Global,
                        &manager,
                        &store,
                        &import_local_tx,
                        &to_output_tx,
                    )
                    .await;

                    if let Err(ref err) = result {
                        warn!("failed to repair spaces: {}", err);
                    }
                }
            }
        }
    })
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)] // TODO: Reduce size of spaces error types.
pub enum RepairError {
    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error(transparent)]
    SpacesManager(#[from] SpacesManagerError),

    #[error("could not send to processor pipeline: {0}")]
    SendToProcessor(String),

    #[error("import ready channel broken")]
    Recv(#[from] RecvError),

    #[error("application send channel broken")]
    AppSend,
}
