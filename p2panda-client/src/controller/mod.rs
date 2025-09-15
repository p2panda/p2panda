// SPDX-License-Identifier: MIT OR Apache-2.0

mod consumer;
#[allow(clippy::module_inception)]
mod controller;

pub use controller::{Controller, ControllerError};
