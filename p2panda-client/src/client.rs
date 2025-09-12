// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::{Stream, ready};
use futures_util::StreamExt;
use p2panda_core::cbor::decode_cbor;
use p2panda_core::{Hash, Operation, PrivateKey};
use pin_project::pin_project;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::controller::{Controller, ControllerError};
use crate::query::{Query, QueryError};
use crate::stream::{StreamEvent, StreamHandler, StreamSubscription};
use crate::{OperationStream, StreamProcessor, StreamProcessorOutput};

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
        B: OperationStream,
    {
        Client {
            private_key: self.private_key.unwrap_or(PrivateKey::new()),
            controller: Controller::new(backend),
        }
    }
}

pub struct Client<B> {
    private_key: PrivateKey,
    controller: Controller<B>,
}

impl<B> Client<B>
where
    B: OperationStream,
{
    pub async fn query<M>(&self, query: Query) -> Result<QueryHandler<B, M>, ClientError<B>>
    where
        M: for<'de> Deserialize<'de> + Send + Sync + 'static,
    {
        let processor = QueryProcessor::<M>::new();
        let handler = StreamHandler::new(self.controller.clone(), processor, query);
        Ok(QueryHandler { handler })
    }
}

#[derive(Debug, Error)]
pub enum ClientError<B>
where
    B: OperationStream,
{
    #[error(transparent)]
    Controller(#[from] ControllerError<B>),
}

// Query
// @TODO: Move all of this somewhere else

pub struct QueryHandler<B, M> {
    handler: StreamHandler<B, QueryProcessor<M>, QueryExtensions, M>,
}

impl<B, M> QueryHandler<B, M>
where
    B: OperationStream,
    M: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    pub async fn publish(&self, message: M) -> Result<Hash, ClientError<B>> {
        todo!()
    }

    pub async fn subscribe(&self) -> Result<QuerySubscription<'_, B, M>, ClientError<B>> {
        let stream = self.handler.subscribe().await.unwrap(); // @TODO

        Ok(QuerySubscription {
            stream,
            _marker: PhantomData,
        })
    }

    pub async fn commit(&self, operation_id: Hash) -> Result<Hash, ClientError<B>> {
        todo!()
    }
}

#[pin_project]
pub struct QuerySubscription<'a, B, M>
where
    M: for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    #[pin]
    stream: StreamSubscription<'a, B, QueryProcessor<M>, QueryExtensions, M>,
    _marker: PhantomData<(B, M)>,
}

impl<'a, B, M> Stream for QuerySubscription<'a, B, M>
where
    for<'de> M: Deserialize<'de> + Send + Sync + 'static,
{
    type Item = Result<
        <QueryProcessor<M> as StreamProcessor>::Output,
        <QueryProcessor<M> as StreamProcessor>::Error,
    >;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // @TODO: Use macro instead?
        let res = ready!(self.stream.poll_next_unpin(cx));
        Poll::Ready(res)
    }
}

// @TODO: Extensions we pre-define here for the users when they use the "query" API. Probably this
// will include "previous".
type QueryExtensions = ();

pub struct QueryProcessor<M> {
    _marker: PhantomData<M>,
}

impl<M> Clone for QueryProcessor<M> {
    fn clone(&self) -> Self {
        Self {
            _marker: self._marker.clone(),
        }
    }
}

impl<M> std::fmt::Debug for QueryProcessor<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryProcessor").finish()
    }
}

impl<M> QueryProcessor<M> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<M> StreamProcessor for QueryProcessor<M>
where
    M: for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    type Error = Infallible;

    type Input = Operation<QueryExtensions>;

    type Output = M;

    async fn process(
        &self,
        input: Self::Input,
    ) -> Result<StreamProcessorOutput<Self::Output>, Self::Error> {
        // @TODO: Properly decode operation
        // @TODO: Introduce as_bytes method in p2panda_core?
        let message: M = decode_cbor(&input.body.unwrap().to_bytes()[..]).unwrap();

        // @TODO: Partial ordering
        Ok(StreamProcessorOutput::Completed(message))
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use futures_core::stream::BoxStream;
    use futures_util::{StreamExt, stream};
    use p2panda_core::cbor::{decode_cbor, encode_cbor};
    use p2panda_core::{
        Body, Extensions, Hash, Header, Operation, PrivateKey, PublicKey, Signature,
    };
    use serde::{Deserialize, Serialize, de::IgnoredAny};

    use crate::{Checkpoint, ClientBuilder, ErasedHeader, ErasedOperation, OperationStream, Query};

    fn erase_extensions<E>(operation: Operation<E>) -> ErasedOperation
    where
        E: Extensions,
    {
        let encoded_header = encode_cbor(&operation.header).expect("failed encoding to cbor");
        let header: ErasedHeader =
            decode_cbor(&encoded_header[..]).expect("failed decoding from cbor");

        Operation {
            header,
            hash: operation.hash,
            body: operation.body,
        }
    }

    #[derive(Default, Clone, Debug, Serialize, Deserialize)]
    struct MyExtensions {
        test: u64,
    }

    #[derive(Default, Debug)]
    struct TestNode {}

    impl OperationStream for TestNode {
        type Error = Infallible;

        async fn subscribe(
            &self,
            query: Query,
            from: Checkpoint,
            live: bool,
        ) -> Result<BoxStream<'_, ErasedOperation>, Self::Error> {
            let private_key = PrivateKey::new();
            let public_key = private_key.public_key();

            let body = {
                let encoded = encode_cbor(&Message(String::from("Hello!"))).unwrap();
                Body::from(encoded)
            };

            let mut header = Header {
                version: 1,
                public_key,
                signature: None,
                payload_size: body.size(),
                payload_hash: Some(body.hash()),
                timestamp: 0,
                seq_num: 0,
                backlink: None,
                previous: vec![],
                extensions: Some(MyExtensions { test: 12 }),
            };

            header.sign(&private_key);

            Ok(stream::iter(vec![erase_extensions(Operation {
                hash: header.hash(),
                header,
                body: Some(body),
            })])
            .boxed())
        }

        async fn publish(&self, operation: ErasedOperation) -> Result<(), Self::Error> {
            todo!()
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct Message(String);

    #[tokio::test]
    async fn it_works() {
        let node = TestNode::default();
        let client = ClientBuilder::new().build(node);

        let handle = client
            .query::<Message>(Hash::new(b"test").into())
            .await
            .unwrap();

        let mut rx = handle.subscribe().await.unwrap();

        while let Some(message) = rx.next().await {
            // ...
        }
    }
}
