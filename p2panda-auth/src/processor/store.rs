// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::traits::{IdentityHandle, OperationId};
use crate::{
    group::GroupCrdtState,
    traits::{Conditions, Operation},
};

use p2panda_core::{Hash, PublicKey};
use p2panda_stream::partial::MemoryStore;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

/// All state material required for ordering and processing group messages.
#[derive(Clone)]
pub struct AuthState<M, A = PublicKey, ID = Hash, C = ()>
where
    A: IdentityHandle,
    ID: OperationId,
    M: Operation<A, ID, C>,
    C: Conditions,
{
    pub crdt: GroupCrdtState<A, ID, M, C>,
    pub orderer: MemoryStore<ID>,
    pub operation_buffer: HashMap<ID, M>,
}

impl<M, A, ID, C> Default for AuthState<M, A, ID, C>
where
    A: IdentityHandle,
    ID: OperationId,
    M: Operation<A, ID, C>,
    C: Conditions,
{
    fn default() -> Self {
        Self {
            crdt: GroupCrdtState::default(),
            orderer: MemoryStore::default(),
            operation_buffer: HashMap::default(),
        }
    }
}

/// Memory store for retrieving and mutating auth state.
///
/// NOTE: this in-memory implementation will be replaced with SQLite stores in the near future.
#[derive(Clone)]
pub struct Store<M, A = PublicKey, ID = Hash, C = ()>
where
    A: IdentityHandle,
    ID: OperationId,
    M: Operation<A, ID, C>,
    C: Conditions,
{
    transaction_lock: Arc<Mutex<()>>,

    #[allow(clippy::type_complexity)]
    state: Arc<Mutex<Option<AuthState<M, A, ID, C>>>>,
}

impl<M, A, ID, C> Store<M, A, ID, C>
where
    A: IdentityHandle,
    ID: OperationId,
    M: Operation<A, ID, C> + Clone,
    C: Conditions,
{
    pub async fn begin_transaction(&self) -> MutexGuard<'_, ()> {
        self.transaction_lock.lock().await
    }

    pub async fn take_state(&self) -> AuthState<M, A, ID, C> {
        self.state.lock().await.take().unwrap_or_default()
    }

    pub async fn set_state(&self, state: AuthState<M, A, ID, C>) {
        *self.state.lock().await = Some(state);
    }

    pub async fn get_state(&self) -> AuthState<M, A, ID, C> {
        self.state
            .lock()
            .await
            .as_ref()
            .cloned()
            .unwrap_or_default()
    }
}

impl<M, A, ID, C> Default for Store<M, A, ID, C>
where
    A: IdentityHandle,
    ID: OperationId,
    M: Operation<A, ID, C>,
    C: Conditions,
{
    fn default() -> Self {
        Self {
            transaction_lock: Arc::new(Mutex::new(())),
            state: Arc::new(Mutex::new(Some(AuthState::default()))),
        }
    }
}
