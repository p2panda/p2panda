// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::Hash;
use p2panda_spaces::manager::GLOBAL_GROUPS_CONTEXT_ID;
use p2panda_spaces::{AuthGroupState, GroupId, SpaceId};
use p2panda_store::groups::GroupsStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::oneshot::Sender;
use tokio::sync::oneshot::error::RecvError;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

use crate::egress::{EgressError, EgressHandle, SubmitError};
use crate::operation::Operation;
use crate::spaces::types::{AuthCapabilities, SpacesStore};

/// Specifies whether all groups should be shared into a space or just a sub-set.
#[derive(Clone, Debug)]
#[allow(unused)]
pub enum GroupsScope {
    Global,
    Partial(Vec<GroupId>),
}

pub type InviteTaskSender = mpsc::UnboundedSender<InviteTaskCommand>;

/// Task for pushing group operations into a space on demand.
///
/// This is needed when:
///
/// 1. We want to add an existing group to a space
/// 2. We want _someone else_ to add a group we know about to a space
/// 
/// The effect of sending these operations into a spaces' egress handle is that they are
/// dispatched to the delivery layer for "live" replication, and the `GroupsHook` will be
/// triggered to perform log associations required for sync.
#[derive(Clone, Debug)]
pub struct InviteTask {
    tx: InviteTaskSender,
}

impl InviteTask {
    /// Spawn invite background task.
    pub fn spawn(store: SqliteStore) -> Self {
        debug!("invite task started");

        let (tx, mut rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;

                    command = rx.recv() => {
                        let Some(command) = command else {
                            // Stop task when all senders were dropped.
                            debug!("invite task ended");
                            break;
                        };

                        match command {
                            InviteTaskCommand::ShareGroups { space_id, egress_handle, scope, reply_tx } => {
                                let result = share_groups(
                                    space_id,
                                    &scope,
                                    &store,
                                    &egress_handle,
                                )
                                .await;

                                if let Err(ref err) = result {
                                    warn!("invite task error: {}", err);
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

    /// Helper method for sharing group operations, based on GroupsScope, with a space.
    pub async fn share(
        &self,
        space_id: SpaceId,
        scope: GroupsScope,
        egress_handle: EgressHandle,
    ) -> Result<bool, InviteError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let command = InviteTaskCommand::ShareGroups {
            space_id,
            egress_handle,
            scope,
            reply_tx,
        };
        self.tx.send(command)?;
        let success = reply_rx.await??;
        Ok(success)
    }

    pub fn command_handle(&self) -> InviteTaskSender {
        self.tx.clone()
    }
}

pub(crate) async fn share_groups(
    space_id: SpaceId,
    scope: &GroupsScope,
    store: &SqliteStore,
    egress_handle: &EgressHandle,
) -> Result<bool, InviteError> {
    let spaces_store = SpacesStore::new(store.clone());

    let groups_y: AuthGroupState<AuthCapabilities> = tx!(
        spaces_store,
        spaces_store
            .get_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID))
            .await?
    )
    .unwrap_or_default();

    let group_ids = match scope {
        GroupsScope::Global => groups_y.groups_global(),
        GroupsScope::Partial(group_ids) => group_ids.clone(),
    };

    // Iterate over operations for all requested groups and their sub-groups in topological order.
    // All operations are sent into the provided space egress handle.
    for id in groups_y.inner.toposort(&group_ids) {
        let Some(operation): Option<Operation> = tx!(store, store.get_operation_tx(&id).await?)
        else {
            warn!("missing expected auth groups operation");
            continue;
        };

        let processed = egress_handle
            .dispatch(operation.into(), space_id.into())
            .await?;
        processed.await?;
    }

    Ok(true)
}

/// Command for space invite task.
#[derive(Debug)]
pub enum InviteTaskCommand {
    ShareGroups {
        space_id: SpaceId,
        egress_handle: EgressHandle,
        scope: GroupsScope,
        reply_tx: Sender<Result<bool, InviteError>>,
    },
}

#[derive(Debug, Error)]
pub enum InviteError {
    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error("could not send to processor pipeline: {0}")]
    SendToProcessor(String),

    #[error(transparent)]
    SendToTask(#[from] SendError<InviteTaskCommand>),

    #[error("import ready channel broken")]
    Recv(#[from] RecvError),

    #[error(transparent)]
    Submit(#[from] SubmitError),

    #[error(transparent)]
    Egress(#[from] EgressError),

    #[error("application send channel broken")]
    AppSend,
}
