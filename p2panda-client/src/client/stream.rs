// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use futures_util::Sink;
use p2panda_core::Hash;

use crate::Subject;
use crate::client::message::Message;

pub struct StreamHandle<M> {
    subject: Subject,
    _marker: PhantomData<M>,
}

impl<M> StreamHandle<M>
where
    M: Message,
{
    pub(crate) fn new(subject: Subject) -> Self {
        Self {
            subject,
            _marker: PhantomData,
        }
    }

    pub async fn publish(&self, _message: M) -> Result<(), StreamError> {
        Ok(())
    }

    pub async fn subscribe(&self) -> Result<StreamSubscription<M>, StreamError> {
        todo!()
    }

    pub async fn commit(&self, _operation_id: Hash) -> Result<Hash, StreamError> {
        todo!()
    }
}

// @TODO
impl<M> Sink<()> for StreamHandle<M> {
    type Error = StreamError;

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

pub struct StreamSubscription<M> {
    _marker: PhantomData<M>,
}

// @TODO
impl<M> Stream for StreamSubscription<M>
where
    M: Message,
{
    type Item = ();

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        todo!()
    }
}

pub enum StreamError {}
