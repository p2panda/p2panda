// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use p2panda_core::traits::ShortFormat;
use p2panda_net::connection_authoriser::ConnectionAuthoriser;
use p2panda_spaces::space::GroupsScope;
use p2panda_spaces::{Event, GroupId, SpaceId, SpacesStoreState};
use p2panda_store::operations::OperationStore;
use p2panda_store::spaces::SpacesStore as SpacesStoreTrait;
use p2panda_store::{SqliteError, SqliteStore, Transaction};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::oneshot::Sender;
use tokio::sync::oneshot::error::RecvError;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, trace, warn};

use crate::operation::Operation;
use crate::spaces::SpacesManagerError;
use crate::spaces::authoriser::update_authoriser;
use crate::spaces::types::{
    AuthCapabilities, InnerSpace, InnerSpaceError, SpacesManager, SpacesStore,
};
use crate::streams::{
    ImportLocalTx, LocalStreamFuture, ToOutputTx, to_stream_event, to_system_event,
};

const REPAIR_FREQUENCY: Duration = Duration::from_secs(1);

pub const DEFAULT_REPAIR_STRATEGY: RepairStrategy = RepairStrategy::Global;

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
/// Only operations for groups added to a space (via a local action or by explicit association) are
/// merged into the space. This results in improved privacy as there is no group "leakage" from the
/// shared state into the space, however it means the initial "discovery" of a new to-be-added group
/// must be solved via another channel (side-channel, dedicated topic, etc.).
///
/// TODO: This initial discovery mechanism is not yet implemented, it may be solved via invite
/// tokens, or manually exporting and then registering a member group. Therefore all spaces use the
/// "Global" strategy for now.
#[derive(Clone, Debug)]
pub enum RepairStrategy {
    Global,
    #[allow(unused)]
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
pub(crate) async fn sync_and_repair_space<M>(
    space_id: SpaceId,
    strategy: &RepairStrategy,
    manager: &SpacesManager,
    store: &SqliteStore,
    import_local_tx: &ImportLocalTx,
    to_output_tx: &ToOutputTx<M>,
    // TODO: Only required until https://github.com/p2panda/p2panda/issues/1362 is resolved.
    connection_authoriser: &ConnectionAuthoriser,
) -> Result<(), RepairError> {
    let spaces_store = SpacesStore::new(store.clone());

    let Some(space) = manager.space(space_id).await? else {
        // This can happen if we didn't receive any space control messages yet.
        trace!(
            node_id = manager.id().fmt_short(),
            space_id = space_id.fmt_short(),
            "space not yet materialised"
        );
        return Ok(());
    };

    // TODO: This is a simple conversion between types, we could even get rid of RepairStrategy in
    // favour of using GroupsScope.
    let scope = match strategy {
        RepairStrategy::Global => GroupsScope::Global,
        RepairStrategy::Partial(_) => unimplemented!(),
    };

    // Identify and send missing group operations to the pipeline.
    send_missing_group_operations(&space, store, &scope, import_local_tx).await?;

    // Incorporate missing group operations into the space.
    let events = repair_space(&space, &spaces_store, &scope, import_local_tx).await?;

    // Update authoriser state.
    //
    // TODO: Only required until https://github.com/p2panda/p2panda/issues/1362 is resolved.
    update_authoriser(connection_authoriser, &events).await;

    // Send resulting enriched space events to the user.
    let events = events
        .into_iter()
        .filter_map(|event| match event {
            p2panda_spaces::Event::Spaces(space_event) => Some(to_stream_event(space_event).into()),
            p2panda_spaces::Event::Groups(group_event) => Some(to_system_event(group_event).into()),
            _ => None,
        })
        .collect();

    to_output_tx
        .send(events)
        .await
        .map_err(|_| RepairError::AppSend)?;

    Ok(())
}

/// Identify group operations which are present in the global groups state but not incorporated
/// into the space and send them to the pipeline.
///
/// We need any running sync sessions to receive these and to associate the group logs with the
/// space topic.
async fn send_missing_group_operations(
    space: &InnerSpace,
    store: &SqliteStore,
    scope: &GroupsScope,
    import_local_tx: &ImportLocalTx,
) -> Result<(), RepairError> {
    let missing_group_messages = space.missing_group_messages(scope).await?;

    if missing_group_messages.is_empty() {
        return Ok(());
    }

    // Collect all missing groups operations. These will be imported into the space and forwarded to
    // live-mode peers.
    let permit = store.begin().await?;

    let mut group_operations = vec![];
    for id in missing_group_messages {
        let Some(operation): Option<Operation> = store.get_operation_tx(&id).await? else {
            warn!("missing expected groups operation");
            continue;
        };

        debug!(id=%operation.hash, seq_num=operation.header.seq_num, "import group operation into space topic");

        group_operations.push(operation)
    }

    store.commit(permit).await?;

    let stream = Box::pin(futures_util::stream::iter(group_operations));
    let (ready_tx, ready_rx) = oneshot::channel::<LocalStreamFuture>();
    import_local_tx
        .send((stream, ready_tx))
        .await
        .map_err(|err| RepairError::SendToProcessor(err.to_string()))?;

    // Await processing of operations to be complete.
    ready_rx.await?;

    Ok(())
}

/// Forge SpaceMembership messages for any group operations missing from the space state.
///
/// Resulting operations are sent to the pipeline.
async fn repair_space(
    space: &InnerSpace,
    store: &SpacesStore,
    scope: &GroupsScope,
    import_local_tx: &ImportLocalTx,
) -> Result<Vec<Event<AuthCapabilities>>, RepairError> {
    let (space_y, spaces_messages, events) = space.repair(scope).await?;
    let operations: Vec<_> = spaces_messages
        .into_iter()
        .map(|message| message.into_operation())
        .collect();

    // If no space messages were forged during repairing then no state change occurred and we can
    // return here.
    if operations.is_empty() {
        return Ok(vec![]);
    };

    let permit = store.begin().await?;

    let space_id = space_y.space_id;
    store
        .set_space_state_tx(&space_id, &SpacesStoreState::from(space_y))
        .await?;

    store.commit(permit).await?;

    // Send all resulting operations into the stream.
    let stream = Box::pin(futures_util::stream::iter(operations));
    let (ready_tx, ready_rx) = oneshot::channel::<LocalStreamFuture>();
    import_local_tx
        .send((stream, ready_tx))
        .await
        .map_err(|err| RepairError::SendToProcessor(err.to_string()))?;

    // Await processing of operations to be complete.
    ready_rx.await?;

    Ok(events)
}

pub type RepairTaskSender = mpsc::UnboundedSender<RepairTaskCommand>;

/// Background task to automatically repair a space.
#[derive(Clone, Debug)]
pub struct RepairTask {
    tx: RepairTaskSender,
}

impl RepairTask {
    /// Spawn repair background task.
    pub fn spawn<M>(
        space_id: SpaceId,
        manager: SpacesManager,
        store: SqliteStore,
        strategy: RepairStrategy,
        import_tx: ImportLocalTx,
        to_output_tx: ToOutputTx<M>,
        // TODO: Only required until https://github.com/p2panda/p2panda/issues/1362 is resolved.
        connection_authoriser: ConnectionAuthoriser,
    ) -> Self
    where
        M: Send + 'static,
    {
        debug!("repair management task started");

        let (tx, mut rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(REPAIR_FREQUENCY);
            loop {
                tokio::select! {
                    biased;

                    _ = interval.tick() => {
                        let result = sync_and_repair_space(
                            space_id,
                            &strategy,
                            &manager,
                            &store,
                            &import_tx,
                            &to_output_tx,
                            &connection_authoriser
                        )
                        .await;

                        if let Err(ref err) = result {
                            warn!("failed to repair spaces: {}", err);
                        }
                    }

                    command = rx.recv() => {
                        let Some(command) = command else {
                            // Stop task when all senders were dropped.
                            debug!("space repair task ended");
                            break;
                        };

                        match command {
                            RepairTaskCommand::Repair(reply_tx) => {
                                let result = sync_and_repair_space(
                                    space_id,
                                    &strategy,
                                    &manager,
                                    &store,
                                    &import_tx,
                                    &to_output_tx,
                                    &connection_authoriser
                                )
                                .await;

                                if let Err(ref err) = result {
                                    warn!("failed to repair spaces: {}", err);
                                }

                                let _ = reply_tx.send(result);

                            },
                        }
                    }
                }
            }
        });

        Self { tx }
    }

    pub async fn repair(&self) -> Result<(), RepairError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RepairTaskCommand::Repair(tx))?;
        rx.await??;
        Ok(())
    }
}

/// Command for space repair task.
#[derive(Debug)]
pub enum RepairTaskCommand {
    /// Repair a space already registered with the task.
    Repair(Sender<Result<(), RepairError>>),
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)] // TODO: Reduce size of spaces error types.
pub enum RepairError {
    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error(transparent)]
    SpacesManager(#[from] SpacesManagerError),

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error("could not send to processor pipeline: {0}")]
    SendToProcessor(String),

    #[error(transparent)]
    SendToTask(#[from] SendError<RepairTaskCommand>),

    #[error("import ready channel broken")]
    Recv(#[from] RecvError),

    #[error("application send channel broken")]
    AppSend,
}
