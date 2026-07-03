// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use futures_util::stream::BoxStream;
use p2panda_core::traits::{Provenance, ShortFormat};
use p2panda_core::{Hash, Topic};
use p2panda_spaces::manager::GLOBAL_GROUPS_CONTEXT_ID;
use p2panda_spaces::{AuthGroupState, SpaceId, SpacesStoreState};
use p2panda_store::groups::GroupsStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::spaces::SpacesStore as SpacesStoreTrait;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, Transaction};
use thiserror::Error;
use tokio::sync::mpsc::{self};
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::RecvError;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::operation::Operation;
use crate::spaces::group_log_id;
use crate::spaces::types::{AuthCapabilities, SpacesArgs, SpacesStore};
use crate::spaces::{SpacesManagerError, types::SpacesManager};
use crate::streams::ExternalStreamFuture;

const REPAIR_DELAY_SECS: u64 = 1;

/// Repairing a space involves incorporating missing groups operations observed on the global groups
/// context but not yet published into the space.
///
/// There are 3 steps to this process:
///
/// 1) re-publish missing groups operations into the space topic
/// 2) create and publish space membership operations for each missing groups operation (only read
///    members can do this)
/// 3) associate missing groups logs with the space topic
///
/// All new messages will be sent into the topic stream to be processed and forwarded to other peers.
pub(crate) async fn repair_space(
    space_id: SpaceId,
    manager: &SpacesManager,
    store: &SqliteStore,
    import_tx: &mpsc::Sender<(
        BoxStream<'static, Operation>,
        oneshot::Sender<ExternalStreamFuture>,
    )>,
) -> Result<bool, RepairError> {
    let spaces_store = SpacesStore::new(store.clone());

    // Identify if this space needs repairing.
    //
    // @TODO: optimize this query by only checking the one space we're concerned with here.
    let space_ids = match manager.spaces_repair_required().await {
        Ok(ids) => ids,
        Err(err) => {
            return Err(err.into());
        }
    };

    // If not return now already.
    if space_ids.is_empty() || !space_ids.contains(&space_id) {
        return Ok(false);
    }

    // Collect all missing groups operations. These will be imported into the space and forwarded
    // to live-mode peers.
    let permit = store.begin().await?;

    let space_y: Option<SpacesStoreState<AuthCapabilities>> =
        spaces_store.get_space_state_tx(&space_id).await?;

    let groups_y: AuthGroupState<AuthCapabilities> = spaces_store
        .get_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID))
        .await?
        .unwrap_or_default();

    let mut groups_operations = vec![];
    for id in groups_y.inner.operations.keys() {
        if let Some(ref y) = space_y
            && y.groups_y.inner.operations.contains_key(id)
        {
            continue;
        }

        let Some(operation): Option<Operation> = store.get_operation_tx(id).await? else {
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
    // @TODO: This method uses transactions internally (eg. in the Forge) and so we can't make
    // everything part of one transaction on this level yet. It isn't a source of bugs though so
    // for now this is ok.
    let mut repaired = manager.repair_spaces(&[space_id]).await?;

    // Forging these spaces messages will also associate any group logs with this space topic.
    let Some((space_y, spaces_messages)) = repaired.pop() else {
        // This can happen if we didn't receive any space control messages yet.
        debug!(
            node_id = manager.id().fmt_short(),
            space_id = space_id.fmt_short(),
            "space not yet materialised"
        );
        return Ok(false);
    };

    // If no space messages were forged during repairing then no state change occurred and we
    // don't need to persist here. This occurs when we are not a _read_ member of the space (yet).
    //
    // @TODO: Once control messages are encrypted it will not be possible for non-read members to
    // receives any control messages and so this logic can be refactored.
    if !spaces_messages.is_empty() {
        let permit = store.begin().await?;

        // @TODO: Once we can process our own spaces messages then we no longer
        // need to persist this state here.
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
    let (ready_tx, ready_rx) = oneshot::channel::<ExternalStreamFuture>();
    import_tx
        .send((stream, ready_tx))
        .await
        .map_err(|err| RepairError::SendToProcessor(err.to_string()))?;

    // Await processing of operations to be complete.
    ready_rx.await?;

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
pub(crate) fn spawn_repair_task(
    topic: Topic,
    manager: SpacesManager,
    store: SqliteStore,
    import_tx: mpsc::Sender<(
        BoxStream<'static, Operation>,
        oneshot::Sender<ExternalStreamFuture>,
    )>,
    mut repair_rx: mpsc::Receiver<oneshot::Sender<Result<bool, RepairError>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let reply_tx = tokio::select! {
                // We received a signal to repair the space.
                msg = repair_rx.recv() => {
                    match msg {
                        Some(reply_tx) => Some(reply_tx),
                        None => {
                            // If the repair_tx is dropped then we can exit this task.
                            return;
                        }
                    }
                }
                // Scheduled repair triggered.
                _ = sleep(Duration::from_secs(REPAIR_DELAY_SECS)) => None,
            };

            // Repair the space.
            let result = repair_space(topic.into(), &manager, &store, &import_tx).await;

            if let Err(ref err) = result {
                warn!("failed to repair spaces: {}", err);
            }

            // Return the result.
            if let Some(reply_tx) = reply_tx {
                let _ = reply_tx.send(result);
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
}
