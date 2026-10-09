// SPDX-License-Identifier: MIT OR Apache-2.0

use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::egress::FromEgress;
use crate::streams::Event;

pub type ToProcessingItem = (Event, oneshot::Sender<Event>);

/// Stream of events which can be consumed by an event processing system.
pub type ProcessFromEgress = FromEgress<ToProcessingItem>;

#[derive(Clone, Debug)]
pub struct ToProcessing {
    tx: mpsc::Sender<ToProcessingItem>,
}

impl ToProcessing {
    pub fn new() -> (Self, ReceiverStream<ToProcessingItem>) {
        let (tx, rx) = mpsc::channel::<ToProcessingItem>(1);
        (Self { tx }, ReceiverStream::new(rx))
    }

    pub async fn publish(
        &self,
        event: Event,
    ) -> Result<oneshot::Receiver<Event>, mpsc::error::SendError<ToProcessingItem>> {
        let (result_tx, result_rx) = oneshot::channel();
        self.tx.send((event, result_tx)).await?;
        Ok(result_rx)
    }
}
