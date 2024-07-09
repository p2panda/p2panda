// SPDX-License-Identifier: AGPL-3.0-or-later

use std::future::Future;

use p2panda_core::{Extensions, Operation};
use thiserror::Error;

use crate::context::Context;
use crate::stream::StreamEvent;

pub type IngestResult<E> = Result<StreamEvent<E>, IngestError>;

pub trait Ingest<E>
where
    E: Extensions,
{
    fn ingest(
        &mut self,
        context: Context,
        operation: &Operation<E>,
    ) -> impl Future<Output = IngestResult<E>>;
}

pub trait IngestBulk<E>
where
    E: Extensions,
{
    fn ingest_bulk(
        &mut self,
        context: Context,
        operation: [Operation<E>],
    ) -> impl Future<Output = IngestResult<E>>;
}

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("{0}")]
    Custom(String),
}
