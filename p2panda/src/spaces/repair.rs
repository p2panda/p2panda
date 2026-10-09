// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(test)]
use std::sync::Arc;

use p2panda_core::traits::ShortFormat;
use p2panda_core::{Hash, Topic};
use p2panda_spaces::manager::GLOBAL_GROUPS_CONTEXT_ID;
use p2panda_spaces::{AuthGroupState, GroupId, SpaceId, SpacesStoreState};
use p2panda_store::groups::GroupsStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::spaces::SpacesStore as SpacesStoreTrait;
use p2panda_store::{SqliteError, SqliteStore, Transaction, tx};
use thiserror::Error;
#[cfg(test)]
use tokio::sync::RwLock;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::oneshot::Sender;
use tokio::sync::oneshot::error::RecvError;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, sleep_until};
use tracing::{debug, warn};

use crate::debouncer::Debouncer;
use crate::egress::{Egress, EgressError, EgressHandle, SubmitError};
use crate::operation::Operation;
use crate::spaces::types::{AuthCapabilities, InnerSpaceError, SpacesManager, SpacesStore};
use crate::spaces::{SpaceEgressError, SpacesManagerError, actor_to_topic, dispatch_spaces_events};

pub type RepairTaskSender = mpsc::UnboundedSender<RepairTaskCommand>;

/// Task for syncing and repairing groups and spaces.
///
/// Syncing and repairing a group or space involves the following steps:
///
/// 1) diff   : compare global and per-stream group/space state to detect any operations the latter is missing
/// 2) sync   : send missing operations into the group/space stream for processing
/// 3) repair : for spaces only, also publish "space membership" messages to integrate the new group operations
#[derive(Clone, Debug)]
pub struct RepairTask {
    tx: RepairTaskSender,

    #[cfg(test)]
    report: RepairTaskReport,
}

impl RepairTask {
    fn new() -> (Self, UnboundedReceiver<RepairTaskCommand>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                tx,
                #[cfg(test)]
                report: Default::default(),
            },
            rx,
        )
    }

    #[cfg(test)]
    fn report(&self) -> RepairTaskReport {
        self.report.clone()
    }
}

impl RepairTask {
    /// Spawn repair background task.
    pub fn spawn(
        manager: SpacesManager,
        store: SqliteStore,
        egress: Egress,
        mut debouncer: Debouncer,
    ) -> Self {
        debug!("repair task started");

        let (task, mut rx) = Self::new();

        {
            #[cfg(test)]
            let report = task.report();

            tokio::spawn(async move {
                // Set when all senders were dropped; a pending repair still runs.
                let mut closed = false;

                loop {
                    let next_run_at = debouncer.next_run_at();

                    if closed && next_run_at.is_none() {
                        debug!("space repair task ended");
                        break;
                    }

                    tokio::select! {
                        // Timer first so a flood of incoming commands can't starve it.
                        biased;

                        _ = sleep_until(next_run_at.unwrap_or_else(Instant::now)), if next_run_at.is_some() => {
                            #[cfg(test)]
                            report.record_run().await;

                            debouncer.clear_pending();
                            let result = sync_and_repair(&manager, &store, &egress).await;

                            if let Err(ref err) = result {
                                #[cfg(test)]
                                report.record_error(err.to_string()).await;

                                warn!("failed to repair spaces: {}", err);
                            }
                        }

                        command = rx.recv(), if !closed => match command {
                            Some(RepairTaskCommand::RepairWithDebounce) => {
                                debouncer.record_trigger(Instant::now());
                            }
                            Some(RepairTaskCommand::Repair(reply)) => {
                            // This run also serves any pending debounced triggers.
                            debouncer.clear_pending();

                            #[cfg(test)]
                            report.record_run().await;
                            let result = sync_and_repair(&manager, &store, &egress).await;

                            if let Err(ref err) = result {
                                warn!("failed to repair spaces: {}", err);
                            }
                                let _ = reply.send(result);
                            }
                            Some(RepairTaskCommand::SyncSpaceWithGroup{ space_id, group_id, reply }) => {
                                let egress_handle = egress.handle();
                                let result = sync_and_repair_space_with_group(&manager, &store, &egress_handle, space_id, Some(group_id)).await;
                                let _ = reply.send(result);
                            }
                            Some(RepairTaskCommand::SyncGroups{ group_id, other_group_ids, reply }) => {
                                let egress_handle = egress.handle();
                                let result = sync_groups(&store, &egress_handle, group_id, &other_group_ids).await;
                                let _ = reply.send(result);
                            }
                            None => closed = true,
                        },
                    }
                }
            });
        };

        task
    }

    /// Sync and repair all currently subscribed spaces and groups.
    pub async fn sync_and_repair_all(&self) -> Result<(), RepairCommandError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RepairTaskCommand::Repair(tx))?;
        rx.await??;
        Ok(())
    }

    /// Send a trigger to sync and repair all spaces and groups with debounce and throttle logic.
    pub fn sync_and_repair_all_debounced(&self) -> Result<(), RepairCommandError> {
        self.tx.send(RepairTaskCommand::RepairWithDebounce)?;
        Ok(())
    }

    /// Sync and repair a space, including a new group in the computation.
    pub async fn sync_and_repair_space_with_group(
        &self,
        space_id: SpaceId,
        group_id: GroupId,
    ) -> Result<(), RepairCommandError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RepairTaskCommand::SyncSpaceWithGroup {
            space_id,
            group_id,
            reply: tx,
        })?;
        rx.await??;
        Ok(())
    }

    /// Sync a single group with a set of other groups.
    pub async fn sync_groups(
        &self,
        group_id: GroupId,
        other_group_ids: Vec<GroupId>,
    ) -> Result<(), RepairCommandError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RepairTaskCommand::SyncGroups {
            group_id,
            other_group_ids,
            reply: tx,
        })?;
        rx.await??;
        Ok(())
    }
}

/// Sync and repair all currently subscribed groups and spaces.
async fn sync_and_repair(
    manager: &SpacesManager,
    store: &SqliteStore,
    egress: &Egress,
) -> Result<(), RepairError> {
    let egress_handle = egress.handle();
    for topic in egress.group_topics().await {
        let Ok(group_id) = GroupId::from_bytes(topic.as_bytes()) else {
            warn!("error deriving group from topic in repair task");
            continue;
        };
        if let Err(err) = sync_group(store, &egress_handle, group_id).await {
            warn!("error repairing group in repair task: {}", err);
        };
    }

    for topic in egress.space_topics().await {
        if let Err(err) = sync_and_repair_space(manager, store, &egress_handle, topic.into()).await
        {
            warn!("error repairing space in repair task: {}", err);
        };
    }

    Ok(())
}

/// Sync and repair a single space.
async fn sync_and_repair_space(
    manager: &SpacesManager,
    store: &SqliteStore,
    egress_handle: &EgressHandle,
    space_id: SpaceId,
) -> Result<(), RepairError> {
    sync_and_repair_space_with_group(manager, store, egress_handle, space_id, None).await
}

/// Repair a space and optionally include an additional non-member group in the computation.
///
/// Including an optional non-member group is required when adding a new group to a space. In this
/// case we need to actively process all operations for the new group _before_ adding them.
///
/// TODO: This step could be moved into p2panda-spaces by adjusting Space::add to return
/// these group messages alongside any newly forged "space membership" messages.
async fn sync_and_repair_space_with_group(
    manager: &SpacesManager,
    store: &SqliteStore,
    egress_handle: &EgressHandle,
    space_id: SpaceId,
    other_group: Option<GroupId>,
) -> Result<(), RepairError> {
    let spaces_store = SpacesStore::new(store.clone());

    let Some(space) = manager.space(space_id).await? else {
        return Ok(());
    };

    // We sync both the space group itself and the optional "other" group.
    let mut group_ids = vec![space.group_id().await?];
    if let Some(id) = other_group {
        group_ids.push(id)
    };

    // Send any groups operations missing from the space into the space stream.
    sync_space(store, egress_handle, space_id, &group_ids).await?;

    // Incorporate the missing operations into the space, this forges new "space membership"
    // operations.
    let output = space.repair(&group_ids).await?;

    // Persist spaces state.
    tx!(spaces_store, {
        spaces_store
            .set_space_state_tx(&space_id, &SpacesStoreState::from(output.space_y))
            .await?;
    });

    dispatch_spaces_events(egress_handle, space_id, output.messages).await?;

    debug!(space_id = space_id.fmt_short(), "space repair success");

    Ok(())
}

/// Sync a group with the global state.
async fn sync_group(
    store: &SqliteStore,
    egress_handle: &EgressHandle,
    group_id: GroupId,
) -> Result<(), SyncError> {
    // Here we just sync the group with itself.
    sync_groups(store, egress_handle, group_id, &[group_id]).await
}

/// Sync a group with any number of other groups.
///
/// Including non-member groups is required when adding a new group to the group. In this case we
/// need to actively process all operations for the new group _before_ adding them.
///
/// TODO: This step could be moved into p2panda-spaces by adjusting Group::add to return required
/// group messages alongside any newly forged ones.
async fn sync_groups(
    store: &SqliteStore,
    egress_handle: &EgressHandle,
    group_id: GroupId,
    sync_with_groups: &[GroupId],
) -> Result<(), SyncError> {
    // Always include the group itself.
    let mut sync_with_groups = sync_with_groups.to_vec();
    sync_with_groups.push(group_id);

    let permit = store.begin().await?;

    let global_groups_y: AuthGroupState<AuthCapabilities> = store
        .get_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID))
        .await?
        .unwrap_or_default();

    let local_groups_y: AuthGroupState<AuthCapabilities> = store
        .get_groups_state_tx(actor_to_topic(group_id).into())
        .await?
        .unwrap_or_default();

    store.commit(permit).await?;

    let ids = global_groups_y.inner.toposort(&sync_with_groups);
    if ids.is_empty() {
        return Ok(());
    }

    send_missing_operations(
        store,
        &local_groups_y,
        actor_to_topic(group_id),
        egress_handle,
        &ids,
    )
    .await?;

    let group_ids = sync_with_groups
        .iter()
        .map(ShortFormat::fmt_short)
        .collect::<Vec<_>>()
        .join(", ");
    debug!(group_id = %group_id, sync_with_groups = %group_ids, operations = ids.len(), "group sync success");
    Ok(())
}

/// Sync a space with any number of groups.
async fn sync_space(
    store: &SqliteStore,
    egress_handle: &EgressHandle,
    space_id: SpaceId,
    sync_with_groups: &[GroupId],
) -> Result<bool, SyncError> {
    let spaces_store = SpacesStore::new(store.clone());
    let permit = spaces_store.begin().await?;

    let global_groups_y: AuthGroupState<AuthCapabilities> = spaces_store
        .get_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID))
        .await?
        .unwrap_or_default();

    let space_groups_y = spaces_store
        .get_space_state_tx(&space_id)
        .await?
        .map(|y: SpacesStoreState<AuthCapabilities>| y.groups_y)
        .unwrap_or_default();

    spaces_store.commit(permit).await?;

    let ids = global_groups_y.inner.toposort(sync_with_groups);
    if ids.is_empty() {
        return Ok(false);
    }

    send_missing_operations(store, &space_groups_y, space_id.into(), egress_handle, &ids).await?;

    let group_ids = sync_with_groups
        .iter()
        .map(ShortFormat::fmt_short)
        .collect::<Vec<_>>()
        .join(", ");
    debug!(space_id = %space_id,  group_ids = group_ids, operations = ids.len(), "space sync success");

    Ok(true)
}

async fn send_missing_operations(
    store: &SqliteStore,
    local_groups_y: &AuthGroupState<AuthCapabilities>,
    topic: Topic,
    egress_handle: &EgressHandle,
    ids: &[Hash],
) -> Result<(), SendOperationsError> {
    for id in ids {
        if local_groups_y.inner.operations.contains_key(id) {
            continue;
        }

        let Some(operation): Option<Operation> = tx!(store, store.get_operation_tx(id).await?)
        else {
            warn!("missing expected auth groups operation");
            continue;
        };

        let processed = egress_handle.dispatch(operation, topic).await?;
        processed.await?;
    }

    Ok(())
}

/// Repair task report only for testing purposes.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
struct RepairTaskReport {
    inner: Arc<RwLock<RepairTaskReportInner>>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct RepairTaskReportInner {
    runs: u32,
    errors: Vec<String>,
}

#[cfg(test)]
impl RepairTaskReport {
    async fn record_run(&self) {
        let mut report = self.inner.write().await;
        report.runs += 1;
    }

    async fn record_error(&self, err: String) {
        let mut report = self.inner.write().await;
        report.errors.push(err);
    }

    async fn runs(&self) -> u32 {
        let report = self.inner.read().await;
        report.runs
    }

    async fn errors(&self) -> Vec<String> {
        let report = self.inner.read().await;
        report.errors.clone()
    }
}

/// Command for space repair task.
#[derive(Debug)]
pub enum RepairTaskCommand {
    /// Repair all groups and spaces.
    Repair(Sender<Result<(), RepairError>>),

    /// Repair all groups and spaces and debounce multiple requests with a max wait.
    RepairWithDebounce,

    /// Sync a group into a space and then repair it.
    SyncSpaceWithGroup {
        space_id: SpaceId,
        group_id: GroupId,
        reply: Sender<Result<(), RepairError>>,
    },

    /// Sync one group into another.
    SyncGroups {
        group_id: GroupId,
        other_group_ids: Vec<GroupId>,
        reply: Sender<Result<(), SyncError>>,
    },
}

#[derive(Debug, Error)]
pub enum RepairCommandError {
    #[error(transparent)]
    SendToTask(#[from] SendError<RepairTaskCommand>),

    #[error("import ready channel broken")]
    Recv(#[from] RecvError),

    #[error(transparent)]
    Repair(#[from] RepairError),

    #[error(transparent)]
    SyncGroups(#[from] SyncError),
}

#[derive(Debug, Error)]
pub enum RepairError {
    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error(transparent)]
    Sync(#[from] SyncError),

    #[error(transparent)]
    SpacesManager(#[from] SpacesManagerError),

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Egress(#[from] SpaceEgressError),
}

#[derive(Debug, Error)]
pub enum SyncError {
    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error(transparent)]
    Send(#[from] SendOperationsError),
}

#[derive(Debug, Error)]
pub enum SendOperationsError {
    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error(transparent)]
    Submit(#[from] SubmitError),

    #[error(transparent)]
    Egress(#[from] EgressError),
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use p2panda_spaces::Config;
    use p2panda_store::SqliteStoreBuilder;

    use crate::Credentials;
    use crate::debouncer::Debouncer;
    use crate::egress::Egress;
    use crate::forge::OperationForge;
    use crate::spaces::spaces_manager;

    use super::RepairTask;

    async fn spawn_task(debouncer: Debouncer) -> RepairTask {
        let store = SqliteStoreBuilder::memory()
            // TODO: Temp fix required due to following issue:
            // https://github.com/p2panda/p2panda/issues/1302
            .max_connections(16)
            .build()
            .await
            .unwrap();

        let credentials = Credentials::generate();

        let forge = OperationForge::new(credentials.clone(), store.clone());

        let manager = spaces_manager(
            forge.clone(),
            credentials.clone(),
            store.clone(),
            Config::default(),
        )
        .unwrap();

        let egress = Egress::new();

        RepairTask::spawn(manager, store, egress.clone(), debouncer)
    }

    #[tokio::test]
    async fn execute_task() {
        let debouncer = Debouncer::default();
        let task = spawn_task(debouncer).await;
        task.sync_and_repair_all().await.unwrap();

        assert_eq!(task.report().runs().await, 1);
        assert!(task.report().errors().await.is_empty());
    }

    #[tokio::test]
    async fn debounce_burst() {
        // Debouncer with long throttle duration which won't be reached in this test.
        let debouncer = Debouncer::new(Duration::from_millis(100), Duration::from_secs(10));
        let task = spawn_task(debouncer).await;

        // continuous burst of tasks should should only cause the task to be executed once.
        for _ in 0..10 {
            task.sync_and_repair_all_debounced().unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Sleep a little after the "burst" so that the task is triggered.
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(task.report().runs().await, 1);
        assert!(task.report().errors().await.is_empty());
    }

    #[tokio::test]
    async fn throttle() {
        let debouncer = Debouncer::new(Duration::from_millis(100), Duration::from_millis(200));
        let task = spawn_task(debouncer).await;

        // If the throttle limit is reached during a burst the task should be triggered even
        // though there is no "quiet" period.
        loop {
            task.sync_and_repair_all_debounced().unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            if task.report().runs().await == 1 {
                break;
            }
        }

        assert_eq!(task.report().runs().await, 1);
        assert!(task.report().errors().await.is_empty());
    }
}
