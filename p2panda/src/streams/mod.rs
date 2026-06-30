// SPDX-License-Identifier: MIT OR Apache-2.0

mod acked;
mod ephemeral_stream;
mod event_stream;
mod external_stream;
mod replay;
mod stream;
mod sync_metrics;

use p2panda_core::{Hash, Topic};
// Useful external types we want to re-export for convenience.
#[doc(no_inline)]
pub use p2panda_core::cbor::DecodeError;

use crate::operation::{Extensions, LogId};

pub use acked::AckedError;
pub(crate) use ephemeral_stream::ephemeral_stream;
pub use ephemeral_stream::{
    EphemeralMessage, EphemeralPublishError, EphemeralStreamPublisher, EphemeralStreamSubscription,
};
pub use event_stream::SystemEvent;
pub(crate) use event_stream::event_stream;
pub use external_stream::ExternalStreamFuture;
pub use replay::{ReplayError, StreamFrom};
pub(crate) use stream::processed_stream;
pub use stream::{
    ImportError, ProcessedOperation, PublishError, PublishFuture, Source, StreamEvent,
    StreamPublisher, StreamSubscription,
};
pub use sync_metrics::{SessionPhase, SyncError};

pub(crate) type Event = crate::processor::Event<LogId, Extensions, Topic>;

pub(crate) type Pipeline = crate::processor::Pipeline<LogId, Extensions, Topic>;

pub(crate) type TaskTracker = crate::processor::TaskTracker<Event, Hash>;
