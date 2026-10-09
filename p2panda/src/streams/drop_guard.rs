// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use tokio::task::JoinHandle;

use crate::egress::EgressRegistration;

/// Aborts the stream tasks and unregisters the stream from egress when the last instance is
/// dropped.
#[derive(Clone, Debug)]
pub struct StreamDropGuard(#[allow(dead_code)] Arc<Inner>);

#[derive(Debug)]
struct Inner {
    tasks: Vec<JoinHandle<()>>,
    _registrations: Vec<EgressRegistration>,
}

impl StreamDropGuard {
    pub fn new(tasks: Vec<JoinHandle<()>>, registrations: Vec<EgressRegistration>) -> Self {
        Self(Arc::new(Inner {
            tasks,
            _registrations: registrations,
        }))
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
