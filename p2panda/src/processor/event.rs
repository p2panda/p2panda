// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use p2panda_core::traits::Digest;
use p2panda_core::{Body, Hash, Header, LogId, Operation, PruneFlag};
use p2panda_stream::ingest::{IngestArgs, IngestError};
use thiserror::Error;

/// Status of an event being processed by a _single_ processor in the pipeline.
#[derive(Clone, Debug, PartialEq)]
pub enum ProcessorStatus<R, F> {
    /// Operation has not been processed yet.
    Pending,

    /// Processor completed this operation successfully. A result might have been attached from
    /// this processor.
    Completed(R),

    /// An error occurred when this operation was processed. A concrete error type is attached.
    Failed(F),
}

/// Single event running through the "event processor" pipeline.
#[derive(Clone, Debug)]
pub struct Event<L, E, TP> {
    /// Input arguments for the processing pipeline.
    input: (Operation<E>, L, TP, PruneFlag),

    /// Status of the "ingest" processor.
    pub(crate) ingest: ProcessorStatus<(), IngestError>,
}

impl<L, E, TP> Event<L, E, TP>
where
    L: LogId,
    TP: Clone,
{
    pub(crate) fn new(
        operation: Operation<E>,
        log_id: L,
        topic: TP,
        prune_flag: PruneFlag,
    ) -> Self {
        Self {
            input: (operation, log_id, topic, prune_flag),
            ingest: ProcessorStatus::Pending,
        }
    }

    pub fn header(&self) -> &Header<E> {
        &self.input.0.header
    }

    pub fn body(&self) -> Option<&Body> {
        self.input.0.body.as_ref()
    }

    /// Returns true if event has been successfully processed by the whole pipeline.
    pub fn is_completed(&self) -> bool {
        matches!(self.ingest, ProcessorStatus::Completed(_))
    }

    /// Returns true if event failed somewhere during processing.
    pub fn is_failed(&self) -> bool {
        matches!(self.ingest, ProcessorStatus::Failed(_))
    }

    /// Returns the error which occurred during a processing failure or `None`.
    pub fn failure_reason(&self) -> Option<EventError> {
        if let ProcessorStatus::Failed(err) = &self.ingest {
            return Some(err.to_owned().into());
        }

        None
    }
}

#[derive(Debug, Error)]
pub enum EventError {
    #[error("ingest processor failed with: {0}")]
    Ingest(#[from] IngestError),
}

impl<L, E, TP> IngestArgs<L, TP, E> for Event<L, E, TP>
where
    L: LogId,
    TP: Clone,
{
    fn log_id(&self) -> L {
        self.input.1.clone()
    }

    fn topic(&self) -> TP {
        self.input.2.clone()
    }

    fn prune_flag(&self) -> bool {
        self.input.3.is_set()
    }

    fn operation(&self) -> impl Borrow<Operation<E>> {
        &self.input.0
    }
}

impl<L, E, TP> Digest<Hash> for Event<L, E, TP> {
    fn hash(&self) -> Hash {
        self.input.0.hash
    }
}

impl<L, E, TP> PartialEq for Event<L, E, TP> {
    fn eq(&self, other: &Self) -> bool {
        self.input.0.hash() == other.input.0.hash()
    }
}

impl<L, E, TP> Eq for Event<L, E, TP> {}
