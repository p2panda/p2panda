// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use p2panda_core::traits::Digest;
use p2panda_core::{
    Body, Extensions, Hash, Header, LogId, Operation, PruneFlag, SeqNum, VerifyingKey,
};
use p2panda_stream::ingest::{IngestArgs, IngestError, IngestResult};
use p2panda_stream::log_prune::{LogPruneArgs, LogPruneError, LogPruneResult};
use p2panda_stream::spaces::{SpacesError, SpacesProcessorArgs, SpacesResult};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::processor::orderer::{OrdererArgs, OrdererError, OrdererMetadata, OrdererResult};
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

/// Single event running through the "event processor" pipeline.
#[derive(Clone, Debug)]
pub struct Event<L, E, TP> {
    /// p2panda Operation.
    pub operation: Operation<E>,

    /// Input arguments for the "ingest" processor.
    pub ingest_args: IngestArgs<L, TP>,

    /// Status of the "ingest" processor.
    pub ingest: ProcessorStatus<IngestResult, IngestError>,

    /// Input arguments for the "orderer" processor.
    pub orderer_args: OrdererArgs,

    /// Status of the "orderer" processor.
    pub orderer: ProcessorStatus<OrdererResult, OrdererError>,

    /// Input arguments for the "log prune" processor.
    pub log_prune_args: LogPruneArgs<VerifyingKey, L, SeqNum>,

    /// Status of the "log prune" processor.
    pub log_prune: ProcessorStatus<LogPruneResult, LogPruneError>,

    /// Input arguments for the "spaces" processor.
    pub spaces_args: SpacesProcessorArgs<AuthCapabilities>,

    /// Status of the "spaces" processor.
    pub spaces: ProcessorStatus<SpacesResult<AuthCapabilities>, SpacesError>,
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
            orderer_args: OrdererArgs::Process {
                dependencies: match &spaces_args {
                    Some(args) => args.dependencies(),
                    None => vec![],
                },
            },
            orderer: ProcessorStatus::Pending,
            log_prune_args: {
                // Do not allow pruning when spaces args are set.
                if prune_flag.is_set() && spaces_args.is_none() {
                    LogPruneArgs::PruneEntriesUntil {
                        author: operation.header.verifying_key,
                        log_id,
                        seq_num: operation.header.seq_num,
                    }
                } else {
                    LogPruneArgs::Ignore
                }
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
            spaces: ProcessorStatus::Pending,
            operation,
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
    pub fn failure_reason(&self) -> Option<ProcessorError> {
        if let ProcessorStatus::Failed(err) = &self.ingest {
            return Some(err.to_owned().into());
        }

        if let ProcessorStatus::Failed(err) = &self.orderer {
            return Some(err.to_owned().into());
        }

        if let ProcessorStatus::Failed(err) = &self.log_prune {
            return Some(err.to_owned().into());
        }

        if let ProcessorStatus::Failed(err) = &self.spaces {
            return Some(err.to_owned().into());
        }

        None
    }

    /// Turn all processor arguments into no-ops by setting them to "ignore".
    ///
    /// This will cause this event to not be processed by _any_ next processors. Usually we want to
    /// call this after an failure happenend.
    pub(crate) fn noop(self) -> Self {
        Self {
            operation: self.operation,
            ingest_args: IngestArgs {
                log_id: self.ingest_args.log_id,
                topic: self.ingest_args.topic,
                prune_flag: false,
            },
            ingest: self.ingest,
            orderer_args: OrdererArgs::Ignore,
            orderer: self.orderer,
            log_prune_args: LogPruneArgs::Ignore,
            log_prune: self.log_prune,
            spaces_args: SpacesProcessorArgs::Ignore,
            spaces: self.spaces,
        }
    }
}

/// Metadata required to construct a new `Event` (excluding the operation).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventMetadata<L, TP> {
    log_id: L,
    topic: TP,
    prune_flag: PruneFlag,
    spaces_args: Option<SpacesArgs>,
    ingest: ProcessorStatus<IngestResult, IngestError>,
}

impl<L, E, TP> OrdererMetadata<E> for Event<L, E, TP>
where
    L: LogId,
    E: Extensions,
    TP: Clone + Serialize + for<'a> Deserialize<'a>,
{
    type Metadata = EventMetadata<L, TP>;

    fn metadata(&self) -> Self::Metadata {
        let log_id = self.ingest_args.log_id.clone();
        let topic = self.ingest_args.topic.clone();
        let prune_flag = PruneFlag::new(self.ingest_args.prune_flag);
        let spaces_args = match &self.spaces_args {
            SpacesProcessorArgs::Ignore => None,
            SpacesProcessorArgs::Process { msg } => Some(msg.args.clone()),
        };
        let ingest = self.ingest.clone();

        EventMetadata {
            log_id,
            topic,
            prune_flag,
            spaces_args,
            ingest,
        }
    }

    fn from_operation(operation: Operation<E>, meta: Self::Metadata) -> Self {
        Self::new(
            operation,
            meta.log_id,
            meta.topic,
            meta.prune_flag,
            meta.spaces_args,
        )
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

impl<L, E, TP> Borrow<OrdererArgs> for Event<L, E, TP>
where
    L: LogId,
    TP: Clone,
{
    fn borrow(&self) -> &OrdererArgs {
        &self.orderer_args
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
