// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use p2panda_core::Operation;

/// Interface expressing required parameters for messages passed into the ingest processor.
pub trait IngestArgs<L, TP, E> {
    fn log_id(&self) -> L;

    fn topic(&self) -> TP;

    fn prune_flag(&self) -> bool;

    fn operation(&self) -> impl Borrow<Operation<E>>;
}
