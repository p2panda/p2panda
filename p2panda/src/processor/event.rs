// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use p2panda_core::traits::Digest;
use p2panda_core::{Body, Hash, Header, LogId, Operation, PruneFlag, PublicKey, SeqNum};
use p2panda_stream::ingest::{IngestArgs, IngestError, IngestResult};
use p2panda_stream::log_prune::{LogPruneArgs, LogPruneError, LogPruneResult};
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
    /// p2panda Operation.
    operation: Operation<E>,

    /// Input arguments for the "ingest" processor.
    ingest_args: IngestArgs<L, TP>,

    /// Status of the "ingest" processor.
    pub(crate) ingest: ProcessorStatus<IngestResult, IngestError>,

    /// Input arguments for the "log prune" processor.
    log_prune_args: LogPruneArgs<PublicKey, L, SeqNum>,

    /// Status of the "log prune" processor.
    pub(crate) log_prune: ProcessorStatus<LogPruneResult, LogPruneError>,
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
            ingest_args: IngestArgs {
                log_id: log_id.clone(),
                topic,
                prune_flag: prune_flag.is_set(),
            },
            ingest: ProcessorStatus::Pending,
            log_prune_args: if prune_flag.is_set() {
                LogPruneArgs::PruneEntriesUntil {
                    author: operation.header.public_key,
                    log_id,
                    seq_num: operation.header.seq_num,
                }
            } else {
                LogPruneArgs::Ignore
            },
            log_prune: ProcessorStatus::Pending,
            operation,
        }
    }

    pub fn header(&self) -> &Header<E> {
        &self.operation.header
    }

    pub fn body(&self) -> Option<&Body> {
        self.operation.body.as_ref()
    }

    /// Returns true if event has been successfully processed by the whole pipeline.
    pub fn is_completed(&self) -> bool {
        matches!(self.ingest, ProcessorStatus::Completed(_))
            && matches!(self.log_prune, ProcessorStatus::Completed(_))
    }

    /// Returns true if event failed somewhere during processing.
    pub fn is_failed(&self) -> bool {
        matches!(self.ingest, ProcessorStatus::Failed(_))
            || matches!(self.log_prune, ProcessorStatus::Failed(_))
    }

    /// Returns the error which occurred during a processing failure or `None`.
    pub fn failure_reason(&self) -> Option<EventError> {
        if let ProcessorStatus::Failed(err) = &self.ingest {
            return Some(err.to_owned().into());
        }

        if let ProcessorStatus::Failed(err) = &self.log_prune {
            return Some(err.to_owned().into());
        }

        None
    }
}

#[derive(Debug, Error)]
pub enum EventError {
    #[error("ingest processor failed with: {0}")]
    Ingest(#[from] IngestError),

    #[error("log_prune processor failed with: {0}")]
    LogPrune(#[from] LogPruneError),
}

impl<L, E, TP> Borrow<Operation<E>> for Event<L, E, TP> {
    fn borrow(&self) -> &Operation<E> {
        &self.operation
    }
}

impl<L, E, TP> Borrow<IngestArgs<L, TP>> for Event<L, E, TP>
where
    L: LogId,
    TP: Clone,
{
    fn borrow(&self) -> &IngestArgs<L, TP> {
        &self.ingest_args
    }
}

impl<L, E, TP> Borrow<LogPruneArgs<PublicKey, L, SeqNum>> for Event<L, E, TP>
where
    L: LogId,
    TP: Clone,
{
    fn borrow(&self) -> &LogPruneArgs<PublicKey, L, SeqNum> {
        &self.log_prune_args
    }
}

impl<L, E, TP> Digest<Hash> for Event<L, E, TP> {
    fn hash(&self) -> Hash {
        self.operation.hash
    }
}

impl<L, E, TP> PartialEq for Event<L, E, TP> {
    fn eq(&self, other: &Self) -> bool {
        self.operation.hash == other.operation.hash
    }
}

impl<L, E, TP> Eq for Event<L, E, TP> {}
