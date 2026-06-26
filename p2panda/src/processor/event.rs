// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use p2panda_core::traits::Digest;
use p2panda_core::{Body, Hash, Header, LogId, Operation, PruneFlag, SeqNum, VerifyingKey};
use p2panda_stream::ingest::{IngestArgs, IngestError, IngestResult};
use p2panda_stream::log_prune::{LogPruneArgs, LogPruneError, LogPruneResult};
use p2panda_stream::orderer::Ordering;
use p2panda_stream::spaces::{SpacesError, SpacesProcessorArgs, SpacesResult};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::processor::orderer::{OrdererError, OrdererResult};
use crate::spaces::types::{AuthCapabilities, SpacesArgs};

/// Status of an event being processed by a _single_ processor in the pipeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ProcessorStatus<R, F> {
    /// Operation has not been processed yet.
    Pending,

    /// Processor completed this operation successfully. A result might have been attached from
    /// this processor.
    Completed(R),

    /// An error occurred when this operation was processed. A concrete error type is attached.
    Failed(F),
}

/// Metadata required to construct a new `Event` (excluding the operation).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct EventMetadata<L, TP> {
    pub(crate) log_id: L,
    pub(crate) topic: TP,
    pub(crate) prune_flag: PruneFlag,
    pub(crate) spaces_args: Option<SpacesArgs>,
    pub(crate) ingest: ProcessorStatus<IngestResult, IngestError>,
}

/// Extract `EventMetadata` from an `Event`.
impl<L, E, TP> From<Event<L, E, TP>> for EventMetadata<L, TP> {
    fn from(event: Event<L, E, TP>) -> Self {
        let log_id = event.ingest_args.log_id;
        let topic = event.ingest_args.topic;
        let prune_flag = PruneFlag::new(event.ingest_args.prune_flag);
        let spaces_args = match event.spaces_args {
            SpacesProcessorArgs::Ignore => None,
            SpacesProcessorArgs::Process { msg } => Some(msg.args),
        };
        let ingest = event.ingest;

        Self {
            log_id,
            topic,
            prune_flag,
            spaces_args,
            ingest,
        }
    }
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

    /// Status of the "orderer" processor.
    pub(crate) orderer: ProcessorStatus<OrdererResult, OrdererError>,

    /// Input arguments for the "log prune" processor.
    log_prune_args: LogPruneArgs<VerifyingKey, L, SeqNum>,

    /// Status of the "log prune" processor.
    pub(crate) log_prune: ProcessorStatus<LogPruneResult, LogPruneError>,

    /// Input arguments for the "spaces" processor.
    spaces_args: SpacesProcessorArgs<AuthCapabilities>,

    /// Status of the "spaces" processor.
    pub(crate) spaces: ProcessorStatus<SpacesResult<AuthCapabilities>, SpacesError>,
}

impl<L, E, TP> Event<L, E, TP>
where
    L: LogId,
{
    pub(crate) fn new(
        operation: Operation<E>,
        log_id: L,
        topic: TP,
        prune_flag: PruneFlag,
        spaces_args: Option<SpacesArgs>,
    ) -> Self {
        Self {
            ingest_args: IngestArgs {
                log_id: log_id.clone(),
                topic,
                prune_flag: prune_flag.is_set(),
            },
            ingest: ProcessorStatus::Pending,
            orderer: ProcessorStatus::Pending,
            // Do not allow pruning when spaces args are set.
            log_prune_args: if prune_flag.is_set() && spaces_args.is_none() {
                LogPruneArgs::PruneEntriesUntil {
                    author: operation.header.verifying_key,
                    log_id,
                    seq_num: operation.header.seq_num,
                }
            } else {
                LogPruneArgs::Ignore
            },
            log_prune: ProcessorStatus::Pending,
            spaces_args: match spaces_args {
                Some(args) => SpacesProcessorArgs::Process {
                    msg: p2panda_spaces::SpacesMessage {
                        id: operation.hash,
                        author: operation.header.verifying_key,
                        args,
                    },
                },
                None => SpacesProcessorArgs::Ignore,
            },
            operation,
            spaces: ProcessorStatus::Pending,
        }
    }

    /// System-level data (append-only log, pruning coordination, etc.) of this operation.
    pub fn header(&self) -> &Header<E> {
        &self.operation.header
    }

    /// Payload of this operation.
    pub fn body(&self) -> Option<&Body> {
        self.operation.body.as_ref()
    }

    /// Returns `true` if event has been successfully processed by the whole pipeline.
    pub fn is_completed(&self) -> bool {
        matches!(self.ingest, ProcessorStatus::Completed(_))
            && matches!(self.orderer, ProcessorStatus::Completed(_))
            && matches!(self.log_prune, ProcessorStatus::Completed(_))
            && matches!(self.spaces, ProcessorStatus::Completed(_))
    }

    /// Returns `true` if event failed somewhere during processing.
    pub fn is_failed(&self) -> bool {
        matches!(self.ingest, ProcessorStatus::Failed(_))
            || matches!(self.orderer, ProcessorStatus::Failed(_))
            || matches!(self.log_prune, ProcessorStatus::Failed(_))
            || matches!(self.spaces, ProcessorStatus::Failed(_))
    }

    /// Returns the error which occurred during a processing failure or `None`.
    pub fn failure_reasons(&self) -> Vec<ProcessorError> {
        let mut reasons = vec![];
        if let ProcessorStatus::Failed(err) = &self.ingest {
            reasons.push(err.to_owned().into());
        }

        if let ProcessorStatus::Failed(err) = &self.orderer {
            reasons.push(err.to_owned().into());
        }

        if let ProcessorStatus::Failed(err) = &self.log_prune {
            reasons.push(err.to_owned().into());
        }

        if let ProcessorStatus::Failed(err) = &self.spaces {
            reasons.push(err.to_owned().into());
        }

        reasons
    }
}

impl<L, E, TP> Ordering<Hash> for Event<L, E, TP> {
    fn dependencies(&self) -> Vec<Hash> {
        match &self.spaces_args {
            SpacesProcessorArgs::Ignore => vec![],
            SpacesProcessorArgs::Process { msg } => msg.args.dependencies(),
        }
    }
}

/// Operation failed during event processing of the system-level pipeline.
///
/// This is likely to come from either processing invalid operations from a broken / malicious node
/// or due to a bug in the Node API.
#[derive(Clone, Debug, Error)]
pub enum ProcessorError {
    #[error("ingest processor failed with: {0}")]
    Ingest(#[from] IngestError),

    #[error("orderer processor failed with: {0}")]
    Orderer(#[from] OrdererError),

    #[error("log_prune processor failed with: {0}")]
    LogPrune(#[from] LogPruneError),

    #[error("spaces processor failed with: {0}")]
    Spaces(#[from] SpacesError),
}

impl<L, E, TP> Borrow<Operation<E>> for Event<L, E, TP> {
    fn borrow(&self) -> &Operation<E> {
        &self.operation
    }
}

impl<L, E, TP> Borrow<Header<E>> for &Event<L, E, TP> {
    fn borrow(&self) -> &Header<E> {
        &self.operation.header
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

impl<L, E, TP> Borrow<LogPruneArgs<VerifyingKey, L, SeqNum>> for Event<L, E, TP>
where
    L: LogId,
    TP: Clone,
{
    fn borrow(&self) -> &LogPruneArgs<VerifyingKey, L, SeqNum> {
        &self.log_prune_args
    }
}

impl<L, E, TP> Borrow<SpacesProcessorArgs<AuthCapabilities>> for Event<L, E, TP>
where
    L: LogId,
    TP: Clone,
{
    fn borrow(&self) -> &SpacesProcessorArgs<AuthCapabilities> {
        &self.spaces_args
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
