// SPDX-License-Identifier: MIT OR Apache-2.0

//! Interfaces to implement and compose event processors on top of data streams with some p2panda
//! implementations out-of-the-box, wrapping existing p2panda crates for causal message ordering,
//! log validation, access control and group encryption CRDTs (`p2panda-spaces`) which might come
//! in handy for many peer-to-peer applications.
//!
//! Developers can "stack up" different stream processors on top of each other and layer them based
//! on the application's needs. Each processor will formulate requirements (in the form of a Rust
//! trait) to the underlying data type; you either "bring your own" or use our `Operation` data type
//! which has most requirements implemented.
//!
//! In p2panda we clearly separate the "event delivery" layer from the "event processing" one. This
//! part of the stack doesn't care what transport was used (pidgeon carriers, usb sticks, internet
//! protocol, LoRa, etc.). Ideally the application should just work fine independent of how the
//! data was delivered, the only part which really matters is how the event stream was _processed_.
//!
//! This stream processor design can be nicely combined with a Pub/Sub system (like `p2panda-net`),
//! scoped and stateful stream controllers (in `p2panda-client`) and storage backends supporting
//! atomic transactions (`p2panda-store`).
//!
//! ## Single-Threaded Design
//!
//! Processors are meant to only be executed within a single thread and do not allow `Send` or
//! `Sync` types. Users need to make sure to run this code in a "local" tokio runtime.
#[cfg(feature = "orderer")]
pub mod orderer;
mod processors;
#[cfg(test)]
mod test_utils;

pub use processors::*;
