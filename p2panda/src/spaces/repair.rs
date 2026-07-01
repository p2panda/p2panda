// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use futures_util::stream::BoxStream;
use p2panda_core::traits::{Provenance, ShortFormat};
use p2panda_core::{Hash, Topic};
use p2panda_spaces::manager::GLOBAL_GROUPS_CONTEXT_ID;
use p2panda_spaces::{AuthGroupState, SpaceId, SpacesStoreState};
use p2panda_store::groups::GroupsStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::spaces::{SpacesStore as SpacesStoreTrait, SqliteSpacesStore};
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, Transaction};
use thiserror::Error;
use tokio::sync::mpsc::{self};
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::RecvError;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::operation::{Extensions, Operation};
use crate::spaces::group_log_id;
use crate::spaces::types::{AuthCapabilities, SpacesArgs};
use crate::spaces::{SpacesManagerError, types::SpacesManager};
use crate::streams::ExternalStreamFuture;

const REPAIR_DELAY_SECS: u64 = 1;

// Repairing a space involves incorporating missing groups operations observed on the global groups
// context but not yet published into the space.
//
// There are 3 steps to this process:
//
// 1) re-publish missing groups operations into the space topic
// 2) create and publish space membership operations for each missing groups operation (only read
//    members can do this)
// 3) associate missing groups logs with the space topic
//
// All new messages will be sent into the topic stream to be processed and forwarded to other peers.
pub(crate) async fn repair_space(
    topic: Topic,
    manager: &SpacesManager,
    store: &SqliteStore,
    import_tx: &mpsc::Sender<(
        BoxStream<'static, Operation>,
        oneshot::Sender<ExternalStreamFuture>,
    )>,
) -> Result<bool, RepairError> {
    let spaces_store = SqliteSpacesStore::<Extensions>::new(store.clone());

    // Identify if this space needs repairing.
    //
    // @TODO: optimize this query by only checking the one space we're concerned with here.
    let space_id = SpaceId::from(topic);
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

    // Attempt to repair the space. As we pass in an array containing a single space id there will
    // be only ever max one result returned.
    //
    // @TODO: This method uses transactions internally (eg. in the Forge) and so we can't make
    // everything part of one transaction on this level yet. It isn't a source of bugs though so
    // for now this is ok.
    let mut repaired = manager.repair_spaces(&[space_id]).await?;

    // Forging these spaces messages will also associate any group logs with this space topic.
    let Some((space_y, spaces_messages)) = repaired.pop() else {
        // This can happen if we didn't receive any space messages yet, or we're not a member with
        // read access.
        debug!(
            node_id = manager.id().fmt_short(),
            space_id = space_id.fmt_short(),
            "space not yet materialised"
        );
        return Ok(false);
    };

    let permit = store.begin().await?;

    // Collect all groups operations in our global context that the space is missing.
    let groups_y: AuthGroupState<AuthCapabilities> = spaces_store
        .get_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID))
        .await?
        .unwrap_or_default();

    let mut groups_operations = vec![];
    for id in groups_y.inner.operations.keys() {
        if space_y.groups_y.inner.operations.contains_key(id) {
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

    // If no _space_ messages were returned from repairing then no state change occurred and we
    // don't need to persist here. This occurs when we are not a _read_ member of the space (yet).
    if !spaces_messages.is_empty() {
        // @TODO: Again, once we can process our own spaces messages then we no longer
        // need to persist this state here.
        let space_id = space_y.space_id;
        spaces_store
            .set_space_state_tx(&space_id, &SpacesStoreState::from(space_y))
            .await?;
    }

    // If there are no messages to send then exit here.
    if spaces_messages.is_empty() && groups_operations.is_empty() {
        store.commit(permit).await?;
        return Ok(false);
    }

    let op_count = groups_operations.len() + spaces_messages.len();
    let operations = groups_operations.into_iter().chain(
        spaces_messages
            .into_iter()
            .map(|message| message.into_operation()),
    );

    // Send the resulting operations into the stream.
    let stream = Box::pin(futures_util::stream::iter(operations));
    let (ready_tx, _ready_rx) = oneshot::channel::<ExternalStreamFuture>();
    import_tx
        .send((stream, ready_tx))
        .await
        .map_err(|err| RepairError::SendToProcessor(err.to_string()))?;

    // Don't await processing otherwise we'd block the stream.
    store.commit(permit).await?;

    debug!(
        node_id = manager.id().fmt_short(),
        space_id = space_id.fmt_short(),
        operations = op_count,
        "space repaired"
    );

    Ok(true)
}

pub(crate) fn spawn_repair_task(
    topic: Topic,
    manager: SpacesManager,
    store: SqliteStore,
    import_tx: mpsc::Sender<(
        BoxStream<'static, Operation>,
        oneshot::Sender<ExternalStreamFuture>,
    )>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(REPAIR_DELAY_SECS)).await;
            if let Err(err) = repair_space(topic, &manager, &store, &import_tx).await {
                warn!("failed to repair spaces: {}", err);
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

    #[error(transparent)]
    Recv(#[from] RecvError),
}
