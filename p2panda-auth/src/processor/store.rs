// SPDX-License-Identifier: MIT OR Apache-2.0
use std::hash::Hash as StdHash;

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
pub struct Store<SID, M, A = PublicKey, ID = Hash, C = ()>
where
    A: IdentityHandle,
    ID: OperationId,
    M: Operation<A, ID, C>,
    C: Conditions,
{
    transaction_lock: Arc<Mutex<()>>,

    #[allow(clippy::type_complexity)]
    states: Arc<Mutex<HashMap<SID, AuthState<M, A, ID, C>>>>,
}

impl<SID, M, A, ID, C> Store<SID, M, A, ID, C>
where
    SID: Copy + Eq + StdHash,
    A: IdentityHandle,
    ID: OperationId,
    M: Operation<A, ID, C> + Clone,
    C: Conditions,
{
    pub async fn begin_transaction(&self) -> MutexGuard<'_, ()> {
        self.transaction_lock.lock().await
    }

    pub async fn set_state(&self, id: &SID, state: AuthState<M, A, ID, C>) {
        let mut states = self.states.lock().await;
        states.insert(*id, state);
    }

    pub async fn get_state(&self, id: &SID) -> Option<AuthState<M, A, ID, C>> {
        self.states.lock().await.get(id).cloned()
    }
}

impl<SID, M, A, ID, C> Default for Store<SID, M, A, ID, C>
where
    A: IdentityHandle,
    ID: OperationId,
    M: Operation<A, ID, C>,
    C: Conditions,
{
    fn default() -> Self {
        Self {
            transaction_lock: Arc::new(Mutex::new(())),
            states: Arc::new(Mutex::new(HashMap::default())),
        }
    }
}
