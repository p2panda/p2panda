// SPDX-License-Identifier: MIT OR Apache-2.0

#[allow(clippy::module_inception)]
mod processor;
mod tasks;

pub use processor::Processor;
pub use tasks::ProcessorTask;
