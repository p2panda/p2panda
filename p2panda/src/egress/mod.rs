// SPDX-License-Identifier: MIT OR Apache-2.0

//! Dispatching locally created operations to event delivery and processing layers.
mod config;
mod delivery;
#[allow(clippy::module_inception)]
mod egress;
mod handle;
mod processing;
mod stream_type;
#[cfg(test)]
mod tests;

pub(crate) use config::{DispatchConfig, EventDeliveryPolicy, EventProcessingPolicy};
pub(crate) use delivery::DeliverFromEgress;
pub(crate) use egress::EgressInner;
pub(crate) use egress::{Egress, EgressRegistration, FromEgress};
pub use handle::DispatchError;
pub(crate) use handle::{DispatchFuture, EgressHandle};
pub(crate) use processing::ProcessFromEgress;
pub(crate) use stream_type::StreamType;

/// Number of operations which can be queued per processing or delivery layer before dispatching
/// applies backpressure.
const BUFFER_SIZE: usize = 128;
