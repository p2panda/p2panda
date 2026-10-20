// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(test)]
use std::sync::Arc;
use std::time::Instant;

use p2panda_core::traits::ShortFormat;
use p2panda_core::{Hash, IdentityError, Topic};
use p2panda_spaces::{AuthGroupState, GroupId, SpaceId, SpacesStoreState, global_groups_id};
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
use tokio::time::sleep_until;
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
/// 1) diff - compare global and per-stream group/space state to detect any operations the latter
///    is missing
/// 2) sync - send missing operations into the group/space stream for processing
/// 3) repair - for spaces only, also publish "space membership" messages to integrate the new
///    group operations
///
/// ## Global groups state
///
/// There is one global state which contains operations for all groups the local node has ever
/// received regardless of the route they arrived. This can be understood as the accumulated local
/// knowledge.
///
/// ## Per-stream groups state
///
/// Group operations are replicated via dedicated group or space topics. For obvious privacy
/// reasons, we only send group operations on a topic stream when the group/space _needs_ them.
/// Initially only operations for the "root" group are required by a stream, as any new group is
/// added as a member, their operations become required as well.
///
/// All group and space streams maintain a "per-stream" state which contains only the operations
/// processed by the stream itself.
///
/// ## Example
///
/// ```text
/// # Global state
///
/// G1: [ ][ ][ ]
/// G2: [ ][ ]
///
/// # Stream G1 state
///
/// G1: [ ][ ][ ]
///
/// # Stream G2 state
///
/// G2: [ ][ ]
/// ```
///
/// Now G2 is added as a member to G1 (this produces a new operation in G1).
///
/// ```text
/// # Global state
///
/// G1: [ ][ ][ ][*] <- new ADD operations
/// G2: [ ][ ]
/// ```
///
/// As well as processing the newly created operation G1 needs to process the existing operations
/// for G2 (from the global state). This is the mechanism described as "sync" above. There is no
/// actual change to G2.
///
/// ```text
/// # Stream G1 state
///
/// G1: [ ][ ][ ][*]
/// G2: [*][*]
///
/// # Stream G2 state
///
/// G2: [ ][ ]
/// ```
///
/// ## Repair task
///
/// The repair task's role is making sure that all streams actually do process all the operations
/// they require. There are two situations where a stream can become out-of-sync (and therefore in
/// need of repair) with the global state.
///
/// 1) changes to required groups occurred since a stream was last subscribed to
/// 2) operations arrived concurrently via a different stream / channel
///
/// ### Example: local change since last subscribe
///
/// We subscribe to G2 and make a membership change.
///
/// ```text
/// # Stream G2 state
///
/// G2: [ ][ ][*]
/// ```
/// We now subscribe to G1. As G2 is a member of G1, we need to process the new operation.
///
/// ```text
/// # Stream G1 state
///
/// G1: [ ][ ][ ][ ]
/// G2: [ ][ ][*]
/// ```
/// ### Example: remote change sent on different stream
///
/// We are subscribed to G1 and G2. A remote makes a change to G2 which we receive and process on
/// G2 stream.
///
/// ```text
/// # Stream G2 state
///
/// G2: [ ][ ][ ][*]
/// ```
/// We are also subscribed to G1 stream which _must_ also process this operation.
///
/// ```text
/// # Stream G1 state
///
/// G1: [ ][ ][ ][ ]
/// G2: [ ][ ][ ][*]
/// ```
///
/// Detecting and dispatching these required operations to the correct streams is the job of the
/// repair task and can be achieved by comparing the "per-stream" state to the "global" state.
///
/// ## Node-Node Sync
///
/// In order for nodes to sync over a topic an association between the topic and all required logs
/// must be made. This job is done in the per-stream pipeline and is achieved by observing
/// operations being processed and associating their logs with the stream topic. By pushing
/// required operations into a stream we are also making sure that the correct log associations
/// are made so that any new logs are included in future sync sessions.
///
/// ## Spaces
///
/// Every space is backed by a single group and membership changes in this group must be reflected
/// by the key-agreement protocol. Key agreement material is published on a SpaceMembership
/// message every time a group control message is processed on a space. When the repair task
/// identifies "missing" groups operations it also forges and dispatches these space control
/// messages.
///
/// ## Repair Frequency
///
/// A trigger is sent to the repair task whenever a group or space is subscribed to, or from the
/// RepairHook whenever it observes a group operation on it's stream. The repair task debounces
/// triggers coming from the RepairHook and applies a throttle to avoid blocking at consistently
/// busy periods.
///
/// It is important to repair _before_ subscribing to a group/space so that any changes since last
/// subscription can be integrated before any _new_ changes are performed locally. This ensures
/// the latest known state is always operated against.
///
/// The debouncer is important to avoid the specific issue of too-eager publishing of
/// SpaceMembership messages to a space. The example being that a remote has changed group state
/// and correctly forged the SpaceMembership message, we receive the groups control message and
/// immediately "repair" ourselves even though the SpaceMembership message from the remote is
/// already in our processing queue.
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

                        _ = sleep_until(next_run_at.unwrap_or_else(Instant::now).into()), if next_run_at.is_some() => {
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
                            Some(RepairTaskCommand::RepairDebounced) => {
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
                            Some(RepairTaskCommand::RepairSpace { space_id, reply }) => {
                                let egress_handle = egress.handle();
                                let result = repair_space(&manager, &store, &egress_handle, space_id, None).await;
                                let _ = reply.send(result);
                            }
                            Some(RepairTaskCommand::RepairSpaceWith { space_id, include, reply }) => {
                                let egress_handle = egress.handle();
                                let result = repair_space(&manager, &store, &egress_handle, space_id, Some(include)).await;
                                let _ = reply.send(result);
                            }
                            Some(RepairTaskCommand::RepairGroup { group_id, reply }) => {
                                let egress_handle = egress.handle();
                                let result = sync_group(&store, &egress_handle, group_id, &[]).await;
                                let _ = reply.send(result);
                            }
                            Some(RepairTaskCommand::RepairGroupWith { group_id, include, reply }) => {
                                let egress_handle = egress.handle();
                                let result = sync_group(&store, &egress_handle, group_id, &include).await;
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
    pub async fn repair_all(&self) -> Result<(), RepairCommandError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RepairTaskCommand::Repair(tx))?;
        rx.await??;
        Ok(())
    }

    /// Send a trigger to sync and repair all spaces and groups with debounce and throttle logic.
    pub fn repair_all_debounced(&self) -> Result<(), RepairCommandError> {
        self.tx.send(RepairTaskCommand::RepairDebounced)?;
        Ok(())
    }

    /// Repair a space.
    pub async fn repair_space(&self, space_id: SpaceId) -> Result<(), RepairCommandError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RepairTaskCommand::RepairSpace {
            space_id,
            reply: tx,
        })?;
        rx.await??;
        Ok(())
    }

    /// Repair a space and include a "new" non-member group which should be incorporated ready to
    /// be added as a member later.
    pub async fn repair_space_with(
        &self,
        space_id: SpaceId,
        include: GroupId,
    ) -> Result<(), RepairCommandError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RepairTaskCommand::RepairSpaceWith {
            space_id,
            include,
            reply: tx,
        })?;
        rx.await??;
        Ok(())
    }

    /// Repair a group.
    pub async fn repair_group(&self, group_id: GroupId) -> Result<(), RepairCommandError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RepairTaskCommand::RepairGroup {
            group_id,
            reply: tx,
        })?;
        rx.await??;
        Ok(())
    }

    /// Repair a group and include a set of "new" non-member groups which should be incorporated
    /// ready to be added as a member later.
    pub async fn repair_group_with(
        &self,
        group_id: GroupId,
        include: &[GroupId],
    ) -> Result<(), RepairCommandError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RepairTaskCommand::RepairGroupWith {
            group_id,
            include: include.to_vec(),
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
        let group_id = GroupId::from_bytes(topic.as_bytes())?;

        // In most cases we don't introduce a new group when syncing a group with the global
        // state. For example when incorporating changes which occurred since last subscribing to
        // a group or when changes arrive from a remote via a different stream. In this case we
        // don't need to pass in an other to repair as the groups id is all that's needed to
        // perform the diff with the global state. For this reason `include` is an empty array
        // here.
        sync_group(store, &egress_handle, group_id, &[]).await?;
    }

    for topic in egress.space_topics().await {
        // In most cases we don't introduce a new group when repairing space. For example when
        // incorporating changes which occurred since last subscribing to a space or when changes
        // arrive from a remote via a different stream. In this case we don't need to pass in an
        // other to repair as the spaces' group id is all that's needed to perform the diff with
        // the global state. For this reason `include` is None here.
        repair_space(manager, store, &egress_handle, topic.into(), None).await?
    }

    Ok(())
}

/// Repair a space and optionally include an additional non-member group in the computation.
///
/// Including an optional non-member group is required when adding a new group to a space. In this
/// case we need to actively process all operations for the new group _before_ adding them.
async fn repair_space(
    manager: &SpacesManager,
    store: &SqliteStore,
    egress_handle: &EgressHandle,
    space_id: SpaceId,
    include: Option<GroupId>,
) -> Result<(), RepairError> {
    // Only members with read access or higher can repair spaces.
    if !manager.has_joined_space(space_id).await.map_err(Box::new)? {
        return Ok(());
    }

    let space = manager.space(space_id).created().await.map_err(Box::new)?;

    // We sync both the space group itself and the optional "other" group.
    let mut group_ids = vec![space.group_id().await.map_err(Box::new)?];
    if let Some(id) = include {
        group_ids.push(id)
    };

    // Send any groups operations missing from the space into the space stream.
    let need_repair = sync_space(store, egress_handle, space_id, &group_ids).await?;
    if !need_repair {
        // If no new group operations were discovered we can return early here.
        debug!(space_id = space_id.fmt_short(), "no repair required");
        return Ok(());
    }

    // Incorporate the missing operations into the space, this forges new "space membership"
    // operations.
    let output = space.repair(&group_ids).await.map_err(Box::new)?;

    dispatch_spaces_events(egress_handle, space_id, output.messages).await?;

    debug!(space_id = space_id.fmt_short(), "space repair success");

    Ok(())
}

/// Sync a group with any number of other groups.
///
/// Including non-member groups is required when adding a new group to the group. In this case we
/// need to actively process all operations for the new group _before_ adding them.
///
/// TODO: The need to include other group ids could be removed from the API here if we adjust
/// p2panda-spaces Group::add to return required group messages alongside any newly forged ones.
/// https://github.com/p2panda/p2panda/issues/1532.
async fn sync_group(
    store: &SqliteStore,
    egress_handle: &EgressHandle,
    group_id: GroupId,
    include: &[GroupId],
) -> Result<(), SyncError> {
    // Always include the group itself.
    let mut include = include.to_vec();
    include.push(group_id);

    let permit = store.begin().await?;

    let global_groups_y: AuthGroupState<AuthCapabilities> = store
        .get_groups_state_tx(global_groups_id())
        .await?
        .unwrap_or_default();

    let local_groups_y: AuthGroupState<AuthCapabilities> = store
        .get_groups_state_tx(actor_to_topic(group_id).into())
        .await?
        .unwrap_or_default();

    store.commit(permit).await?;

    let ids = global_groups_y.inner.toposort(&include);
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

    let group_ids = include
        .iter()
        .map(ShortFormat::fmt_short)
        .collect::<Vec<_>>()
        .join(", ");
    debug!(group_id = %group_id, include = %group_ids, operations = ids.len(), "group sync success");
    Ok(())
}

/// Sync a space with any number of groups.
///
/// We need to include other non-member groups in the sync target when adding new members (which
/// are themselves groups) to the space. This ensures that all operations for these new
/// "dependencies" are met before actually adding the member.
///
/// TODO: The need to include other group ids could be removed from the API here if we adjust
/// p2panda-spaces Group::add to return required group messages alongside any newly forged ones
/// https://github.com/p2panda/p2panda/issues/1532.
async fn sync_space(
    store: &SqliteStore,
    egress_handle: &EgressHandle,
    space_id: SpaceId,
    include: &[GroupId],
) -> Result<bool, SyncError> {
    let spaces_store = SpacesStore::new(store.clone());

    let permit = spaces_store.begin().await?;

    let global_groups_y: AuthGroupState<AuthCapabilities> = spaces_store
        .get_groups_state_tx(global_groups_id())
        .await?
        .unwrap_or_default();

    let space_groups_y = spaces_store
        .get_space_state_tx(&space_id)
        .await?
        .map(|y: SpacesStoreState<AuthCapabilities>| y.groups_y)
        .unwrap_or_default();

    spaces_store.commit(permit).await?;

    let ids = global_groups_y.inner.toposort(include);
    if ids.is_empty() {
        return Ok(false);
    }

    send_missing_operations(store, &space_groups_y, space_id.into(), egress_handle, &ids).await?;

    let include = include
        .iter()
        .map(ShortFormat::fmt_short)
        .collect::<Vec<_>>()
        .join(", ");
    debug!(space_id = %space_id,  include = include, operations = ids.len(), "space sync success");

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
    RepairDebounced,

    /// Repair a space.
    RepairSpace {
        space_id: SpaceId,
        reply: Sender<Result<(), RepairError>>,
    },

    /// Repair a group.
    RepairGroup {
        group_id: GroupId,
        reply: Sender<Result<(), SyncError>>,
    },

    /// Repair a space and include a "new" non-member group which should be incorporated ready to
    /// be added as a member later.
    RepairSpaceWith {
        space_id: SpaceId,
        include: GroupId,
        reply: Sender<Result<(), RepairError>>,
    },

    /// Repair a group and include a set of "new" non-member groups which should be incorporated
    /// ready to be added as a member later.
    RepairGroupWith {
        group_id: GroupId,
        include: Vec<GroupId>,
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
    SyncGroup(#[from] SyncError),
}

#[derive(Debug, Error)]
pub enum RepairError {
    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error(transparent)]
    Sync(#[from] SyncError),

    #[error(transparent)]
    Manager(#[from] Box<SpacesManagerError>),

    #[error(transparent)]
    Space(#[from] Box<InnerSpaceError>),

    #[error(transparent)]
    Egress(#[from] SpaceEgressError),

    #[error(transparent)]
    Identity(#[from] IdentityError),
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
        task.repair_all().await.unwrap();

        assert_eq!(task.report().runs().await, 1);
        assert!(task.report().errors().await.is_empty());
    }

    #[tokio::test]
    async fn debounce_burst() {
        // Debouncer with long throttle duration which won't be reached in this test.
        let debouncer = Debouncer::new(Duration::from_millis(100), Duration::from_secs(10));
        let task = spawn_task(debouncer).await;

        // continuous burst of tasks should only cause the task to be executed once.
        for _ in 0..10 {
            task.repair_all_debounced().unwrap();
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
            task.repair_all_debounced().unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            if task.report().runs().await == 1 {
                break;
            }
        }

        assert_eq!(task.report().runs().await, 1);
        assert!(task.report().errors().await.is_empty());
    }
}
