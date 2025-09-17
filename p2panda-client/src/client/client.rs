// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;

use crate::client::ephemeral_stream::EphemeralStreamHandle;
use crate::client::message::Message;
use crate::client::stream::StreamHandle;
use crate::connector::Connector;
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

    pub fn build<C>(self, connector: C) -> Client<C>
    where
        C: Connector,
    {
        let controller = Controller::new(connector);
        Client {
            private_key: self.private_key.unwrap_or_default(),
            controller,
        }
    }
}

pub struct Client<C>
where
    C: Connector,
{
    private_key: PrivateKey,
    controller: Controller<C>,
}

impl<C> Client<C>
where
    C: Connector,
{
    pub fn stream<M>(&self, subject: Subject) -> Result<StreamHandle<M, C>, ClientError>
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
