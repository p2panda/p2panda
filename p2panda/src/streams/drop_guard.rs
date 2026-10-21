// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use tokio::task::JoinHandle;

#[derive(Clone, Debug)]
pub struct StreamDropGuard(#[allow(dead_code)] Arc<Inner>);

#[derive(Debug)]
struct Inner(Vec<JoinHandle<()>>);

impl StreamDropGuard {
    pub fn new(handles: Vec<JoinHandle<()>>) -> Self {
        Self(Arc::new(Inner(handles)))
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        for handle in &self.0 {
            handle.abort();
        }
    }
}
