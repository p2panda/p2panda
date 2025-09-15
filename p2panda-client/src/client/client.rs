// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;

use crate::client::ephemeral_stream::EphemeralStreamHandle;
use crate::client::message::Message;
use crate::client::stream::StreamHandle;
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

    pub fn build(self) -> Client {
        Client {
            private_key: self.private_key.unwrap_or(PrivateKey::new()),
        }
    }
}

pub struct Client {
    private_key: PrivateKey,
}

impl Client {
    pub fn stream<M>(&self, subject: Subject) -> Result<StreamHandle<M>, ClientError>
    where
        M: Message,
    {
        Ok(StreamHandle::<M>::new(subject))
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
