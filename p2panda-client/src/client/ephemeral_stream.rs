// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use futures_util::Sink;

use crate::TopicId;
use crate::client::message::Message;

// @TODO: Implement this.
pub struct EphemeralStreamHandle<M> {
    #[allow(dead_code)]
    topic_id: TopicId,
    _marker: PhantomData<M>,
}

impl<M> EphemeralStreamHandle<M>
where
    M: Message,
{
    pub(crate) fn new(topic_id: TopicId) -> Self {
        Self {
            topic_id,
            _marker: PhantomData,
        }
    }

    pub async fn publish(&self, _message: M) -> Result<(), EphemeralStreamError> {
        Ok(())
    }

    pub async fn subscribe(&self) -> Result<EphemeralStreamSubscription<M>, EphemeralStreamError> {
        todo!()
    }
}

// @TODO
impl<M> Sink<()> for EphemeralStreamHandle<M> {
    type Error = EphemeralStreamError;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        todo!()
    }

    fn start_send(self: Pin<&mut Self>, _item: ()) -> Result<(), Self::Error> {
        todo!()
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        todo!()
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        todo!()
    }
}

pub struct EphemeralStreamSubscription<M> {
    _marker: PhantomData<M>,
}

// @TODO
impl<M> Stream for EphemeralStreamSubscription<M>
where
    M: Message,
{
    type Item = ();

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        todo!()
    }
}

pub enum EphemeralStreamError {}
