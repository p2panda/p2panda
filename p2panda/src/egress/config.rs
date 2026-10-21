// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::Topic;

/// Configure where operations should be dispatched to.
#[derive(Clone, Default, Debug)]
pub struct DispatchConfig {
    /// Determines if and how the operation is sent to the event delivery layer.
    pub delivery: EventDeliveryPolicy,

    /// Determines if an operation is sent to an event processing pipeline for one or more topic
    /// streams.
    pub processing: EventProcessingPolicy,
}

impl DispatchConfig {
    pub fn topic(topic: Topic) -> Self {
        Self {
            delivery: EventDeliveryPolicy::Topic(topic),
            processing: EventProcessingPolicy::Topic(topic),
        }
    }
}

/// Determines if and how the operation is pushed to the event delivery layer.
///
/// If no active event delivery channels are available for the given scope, this can be a no-op.
#[derive(Clone, Default, Debug)]
pub enum EventDeliveryPolicy {
    /// Do not immediately push operation to any event delivery layer.
    ///
    /// This option does not necessarily control if the operation will _never_ be read by the
    /// network. It merely determines if a networking layer will receive the operation _immediately_
    /// (for example to eagerly push it to all active nodes, flood it in the mesh, etc.). Event
    /// delivery solutions usually have access to the database of all operations and will query the
    /// operation latest from there.
    #[default]
    Disabled,

    /// Push operation to event delivery channel concerned with this topic (if it exists).
    Topic(Topic),

    /// Push operation to all channels which are currently exchanging over encrypted spaces.
    OnlySpaces,
}

/// Determines if an operation is processed by an event processing pipeline for one or more topic
/// streams.
///
/// If no active event processing is available for the given scope, this can be a no-op.
#[derive(Clone, Default, Debug)]
pub enum EventProcessingPolicy {
    /// Do not process operation immediately.
    ///
    /// Note that this option will not block processing entirely for this operation. It will be
    /// processed next time a related topic stream is established and will be handled as part of the
    /// replay (since it wasn't acked yet).
    #[default]
    Disabled,

    /// Process operation in pipeline for a particular topic (if active stream exists).
    Topic(Topic),
}
