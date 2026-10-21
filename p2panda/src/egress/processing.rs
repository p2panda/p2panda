// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};
use p2panda_core::Topic;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::egress::EgressInner;
use crate::streams::Event;

pub type ToProcessingItem = (Event, oneshot::Sender<()>);

/// Stream of evens which can be consumed by an event processing system.
#[derive(Clone, Debug)]
pub struct ToProcessing {
    tx: mpsc::Sender<ToProcessingItem>,
}

#[derive(Debug)]
pub struct ProcessFromEgress {
    topic: Topic,
    rx: ReceiverStream<ToProcessingItem>,
    inner: Arc<RwLock<EgressInner>>,
}

impl Stream for ProcessFromEgress {
    type Item = ToProcessingItem;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_next_unpin(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.rx.size_hint()
    }
}

impl ToProcessing {
    pub fn new(inner: Arc<RwLock<EgressInner>>, topic: Topic) -> (Self, ProcessFromEgress) {
        let (tx, rx) = mpsc::channel::<ToProcessingItem>(1);

        (
            Self { tx },
            ProcessFromEgress {
                inner,
                topic,
                rx: ReceiverStream::new(rx),
            },
        )
    }

    pub async fn publish(
        &self,
        event: Event,
    ) -> Result<oneshot::Receiver<()>, mpsc::error::SendError<ToProcessingItem>> {
        let (result_tx, result_rx) = oneshot::channel();
        self.tx.send((event, result_tx)).await?;
        Ok(result_rx)
    }
}

impl Drop for ProcessFromEgress {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.remove_processing(&self.topic);
        }
    }
}
