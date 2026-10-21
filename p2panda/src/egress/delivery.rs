// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};
use p2panda_core::Topic;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::egress::EgressInner;
use crate::operation::Operation;

pub type ToDeliveryItem = (Operation, oneshot::Sender<()>);

/// Stream of evens which can be consumed by an event delivery system.
#[derive(Clone, Debug)]
pub struct ToDelivery {
    tx: mpsc::Sender<ToDeliveryItem>,
}

#[derive(Debug)]
pub struct DeliverFromEgress {
    topic: Topic,
    rx: ReceiverStream<ToDeliveryItem>,
    inner: Arc<RwLock<EgressInner>>,
}

impl Stream for DeliverFromEgress {
    type Item = ToDeliveryItem;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_next_unpin(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.rx.size_hint()
    }
}

impl Drop for DeliverFromEgress {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.remove_delivery(&self.topic);
        }
    }
}

impl ToDelivery {
    pub fn new(inner: Arc<RwLock<EgressInner>>, topic: Topic) -> (Self, DeliverFromEgress) {
        let (tx, rx) = mpsc::channel(1);

        (
            Self { tx },
            DeliverFromEgress {
                topic,
                inner,
                rx: ReceiverStream::new(rx),
            },
        )
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
