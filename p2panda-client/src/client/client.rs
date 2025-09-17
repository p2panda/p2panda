// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;

use crate::backend::Backend;
use crate::client::ephemeral_stream::EphemeralStreamHandle;
use crate::client::message::Message;
use crate::client::stream::StreamHandle;
use crate::controller::Controller;
use crate::{Subject, TopicId};

pub struct ClientBuilder {
    private_key: Option<PrivateKey>,
}

impl ClientBuilder {
    pub fn new() -> Self {
        Self { private_key: None }
    }

    // @TODO: Have a "credentials store" instead?
    pub fn private_key(mut self, private_key: PrivateKey) -> Self {
        self.private_key = Some(private_key);
        self
    }

    pub fn build<B>(self, backend: B) -> Client<B>
    where
        B: Backend,
    {
        let controller = Controller::new(backend);
        Client {
            private_key: self.private_key.unwrap_or_default(),
            controller,
        }
    }
}

pub struct Client<B>
where
    B: Backend,
{
    private_key: PrivateKey,
    controller: Controller<B>,
}

impl<B> Client<B>
where
    B: Backend,
{
    pub fn stream<M>(&self, subject: Subject) -> Result<StreamHandle<M, B>, ClientError>
    where
        M: Message,
    {
        Ok(StreamHandle::new(subject, self.controller.clone()))
    }

    pub fn ephemeral_stream<M>(
        &self,
        topic_id: TopicId,
    ) -> Result<EphemeralStreamHandle<M>, ClientError>
    where
        M: Message,
    {
        Ok(EphemeralStreamHandle::<M>::new(topic_id))
    }
}

pub enum ClientError {}
