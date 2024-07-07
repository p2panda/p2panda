// SPDX-License-Identifier: AGPL-3.0-or-later

use std::rc::Rc;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Context {
    inner: Rc<ContextInner>,
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(ContextInner {}),
        }
    }
}

#[derive(Debug)]
struct ContextInner {}
