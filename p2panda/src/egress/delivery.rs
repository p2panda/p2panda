// SPDX-License-Identifier: MIT OR Apache-2.0

use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::egress::FromEgress;
use crate::operation::Operation;

pub type ToDeliveryItem = (Operation, oneshot::Sender<()>);

/// Stream of operations which can be published into an event delivery system, usually for "live
/// mode" / eager-push into a network.
pub type DeliverFromEgress = FromEgress<ToDeliveryItem>;

#[derive(Clone, Debug)]
pub struct ToDelivery {
    tx: mpsc::Sender<ToDeliveryItem>,
}

impl ToDelivery {
    pub fn new() -> (Self, ReceiverStream<ToDeliveryItem>) {
        let (tx, rx) = mpsc::channel(1);
        (Self { tx }, ReceiverStream::new(rx))
    }

    pub async fn publish(
        &self,
        operation: Operation,
    ) -> Result<oneshot::Receiver<()>, mpsc::error::SendError<ToDeliveryItem>> {
        let (result_tx, result_rx) = oneshot::channel();
        self.tx.send((operation, result_tx)).await?;
        Ok(result_rx)
    }
}
