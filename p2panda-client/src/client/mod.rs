// SPDX-License-Identifier: MIT OR Apache-2.0

#[allow(clippy::module_inception)]
mod client;
mod ephemeral_stream;
mod message;
mod stream;

pub use client::{Client, ClientBuilder, ClientError};
