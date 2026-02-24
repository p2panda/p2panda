// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use p2panda_core::traits::{Digest, OperationId};
use tokio::sync::{Mutex, Notify, RwLock};

#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct ProcessorTasks<T, ID>(Arc<RwLock<HashMap<ID, Arc<ProcessorTask<T, ID>>>>>);

impl<T, ID> ProcessorTasks<T, ID>
where
    T: Clone + Digest<ID>,
    ID: OperationId,
{
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(HashMap::new())))
    }

    pub async fn queue(&self, id: ID) -> Arc<ProcessorTask<T, ID>> {
        let mut inner = self.0.write().await;

        match inner.get(&id) {
            Some(task) => task.clone(),
            None => {
                let task = Arc::new(ProcessorTask::<T, ID>::new(id));
                inner.insert(id, task.clone());
                task
            }
        }
    }

    pub async fn mark_as_done(&self, id: ID, result: T) {
        let mut inner = self.0.write().await;

        let Some(task) = inner.remove(&id) else {
            return;
        };

        task.mark_as_done(result).await;
    }
}

pub struct ProcessorTask<T, ID> {
    id: ID,
    ready_result: Mutex<Option<T>>,
    ready_signal: Notify,
}

impl<T, ID> ProcessorTask<T, ID>
where
    T: Clone,
{
    pub fn new(id: ID) -> Self {
        Self {
            id,
            ready_result: Mutex::new(None),
            ready_signal: Notify::new(),
        }
    }

    pub async fn mark_as_done(&self, result: T) {
        {
            let mut ready_result = self.ready_result.lock().await;
            *ready_result = Some(result);
        }

        self.ready_signal.notify_waiters();
    }

    pub async fn ready(&self) -> T {
        self.ready_signal.notified().await;
        let mut ready_result = self.ready_result.lock().await;
        ready_result
            .clone()
            .expect("result exists after ready signal was fired")
    }
}
