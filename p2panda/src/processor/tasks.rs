// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::hash::Hash as StdHash;
use std::sync::Arc;

use tokio::sync::{Mutex, Notify, RwLock};

/// Manages tasks for event processing pipelines.
///
/// Tasks can be added to the tracker. Event processors concurrently begin work on these tasks and
/// whenever they've finished they mark the task as "ready". All processes which observe this task
/// will be notified on this "ready" signal.
///
/// Tasks are automatically de-duplicated internally, so whenever multiple processes add the same
/// task, they will look at the same instance and get notified about it's readyness at the same
/// time.
#[derive(Clone, Debug)]
pub struct TaskTracker<T, ID>(Arc<RwLock<HashMap<ID, Task<T, ID>>>>);

impl<T, ID> TaskTracker<T, ID>
where
    T: Clone,
    ID: Copy + Eq + StdHash,
{
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(HashMap::new())))
    }

    /// Returns number of currently pending tasks.
    #[allow(unused)]
    pub async fn len(&self) -> usize {
        let inner = self.0.read().await;
        inner.len()
    }

    /// Registers new "pending" task with an unique identifier.
    ///
    /// The received task is now tracked and can be used to wait for it to be ready.
    pub async fn track(&self, id: ID) -> Task<T, ID> {
        let mut inner = self.0.write().await;

        match inner.get(&id) {
            Some(task) => task.clone(),
            None => {
                let task = Task::<T, ID>::new(id);
                inner.insert(id, task.clone());
                task
            }
        }
    }

    /// Marks a task as "ready" and attaches the result to it.
    ///
    /// Every process which is awaiting the result of this task will be notified.
    pub async fn mark_as_done(&self, id: ID, result: T) {
        let mut inner = self.0.write().await;

        let Some(task) = inner.remove(&id) else {
            return;
        };

        task.mark_as_done(result).await;
    }
}

impl<T, ID> Default for TaskTracker<T, ID>
where
    T: Clone,
    ID: Copy + Eq + StdHash,
{
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct Task<T, ID> {
    id: ID,
    ready_result: Arc<Mutex<Option<T>>>,
    ready_signal: Arc<Notify>,
}

impl<T, ID> PartialEq for Task<T, ID>
where
    ID: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T, ID> Task<T, ID>
where
    T: Clone,
{
    fn new(id: ID) -> Self {
        Self {
            id,
            ready_result: Arc::new(Mutex::new(None)),
            ready_signal: Arc::new(Notify::new()),
        }
    }

    async fn mark_as_done(&self, result: T) {
        {
            let mut ready_result = self.ready_result.lock().await;
            *ready_result = Some(result);
        }

        self.ready_signal.notify_waiters();
    }

    /// Await this task until it is ready and we've received the result.
    pub async fn ready(&self) -> T {
        // Check if an result already exists and return it directly.
        {
            let ready_result = self.ready_result.lock().await;
            if ready_result.is_some() {
                return ready_result
                    .clone()
                    .expect("result exists after ready signal was fired");
            }
        }

        // If not, we wait until we got notified that an result exists.
        self.ready_signal.notified().await;

        let ready_result = self.ready_result.lock().await;
        ready_result
            .clone()
            .expect("result exists after ready signal was fired")
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::TaskTracker;

    #[tokio::test]
    async fn deduplicate_tasks_by_id() {
        let tasks = TaskTracker::<String, usize>::new();
        let task_a = tasks.track(1).await;
        let task_b = tasks.track(1).await; // <-- inserted twice
        let task_c = tasks.track(2).await;
        assert_eq!(tasks.len().await, 2);
        assert_eq!(task_a, task_b);
        assert_ne!(task_a, task_c);
    }

    #[tokio::test]
    async fn notify_all() {
        let tasks = TaskTracker::<String, usize>::new();
        let mut futures = Vec::new();

        const TASK_ID: usize = 1;

        // Simulate 10 concurrent processes queing up the same task at the same time.
        for _ in 0..10 {
            let tasks = tasks.clone();

            let handle = tokio::spawn(async move {
                let task_1 = tasks.track(TASK_ID).await;

                // Make sure only one task is tracked in total.
                assert_eq!(tasks.len().await, 1);

                // Wait for task to be finished.
                task_1.ready().await
            });

            futures.push(handle);
        }

        // Wait for all tasks to spawn before we continue.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Mark the task as done, all processes should finish now and receive the same result.
        tasks.mark_as_done(TASK_ID, "yay, we did it!".into()).await;

        // Check if all processes correctly received the result.
        let result = futures_util::future::join_all(futures).await;
        for i in 0..10 {
            assert_eq!(result[i].as_ref().unwrap(), &"yay, we did it!".to_string());
        }
    }

    #[tokio::test]
    async fn concurrent_removal() {
        let tasks = TaskTracker::<String, usize>::new();
        let task_a = tasks.track(1).await;

        tasks.mark_as_done(1, "yay, we did it".to_string()).await;

        // Marking the same task as "done" a second time is ignored.
        tasks.mark_as_done(1, "yay, we did it".to_string()).await;

        // Result is ready, even if we await it _after_ it was marked as such.
        let result = task_a.ready().await;
        assert_eq!(result, "yay, we did it".to_string());
    }
}
