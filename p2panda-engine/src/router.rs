// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cell::RefCell;
use std::future::IntoFuture;
use std::rc::Rc;

use p2panda_core::{Extensions, Operation};

use crate::boxed::BoxedMiddleware;
use crate::context::Context;
use crate::ingest::{Ingest, IngestError, IngestResult};
use crate::layer::Layer;

#[derive(Clone)]
pub struct Router<E>
where
    E: Extensions,
{
    inner: Rc<RouterInner<E>>,
}

#[derive(Clone)]
struct RouterInner<E>
where
    E: Extensions,
{
    root_path: Option<RefCell<BoxedMiddleware<E>>>,
}

impl<E> RouterInner<E>
where
    E: Extensions,
{
    pub fn new() -> Self {
        Self { root_path: None }
    }
}

impl<E> Default for Router<E>
where
    E: Extensions,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Router<E>
where
    E: Extensions,
{
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RouterInner::new()),
        }
    }

    pub fn route<R>(self, route: R) -> Self
    where
        R: Ingest<E> + Layer<R> + Clone + 'static,
        E: Extensions + 'static,
    {
        self.map_inner(|this| RouterInner {
            root_path: match this.root_path {
                Some(path) => {
                    let path = path.borrow();
                    Some(RefCell::new(path.layer(BoxedMiddleware::new(route))))
                }
                None => Some(RefCell::new(BoxedMiddleware::new(route))),
            },
        })
    }

    fn map_inner<F>(self, f: F) -> Self
    where
        F: FnOnce(RouterInner<E>) -> RouterInner<E>,
    {
        Self {
            inner: Rc::new(f(self.into_inner())),
        }
    }

    fn into_inner(self) -> RouterInner<E> {
        match Rc::try_unwrap(self.inner) {
            Ok(inner) => inner,
            Err(rc) => RouterInner {
                root_path: rc.root_path.clone(),
            },
        }
    }
}

impl<E> Ingest<E> for Router<E>
where
    E: Extensions,
{
    async fn ingest(&mut self, context: Context, operation: &Operation<E>) -> IngestResult<E> {
        match &self.inner.root_path {
            Some(path) => {
                let mut path = path.borrow_mut();
                path.0.ingest(context, operation).into_future().await
            }
            None => Err(IngestError::Custom("no route given".into())),
        }
    }
}

// @TODO: Allow routers to be layered with other routes
