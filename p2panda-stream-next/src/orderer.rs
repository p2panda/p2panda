// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use futures_core::future::BoxFuture;
use thiserror::Error;

use crate::Layer;

pub trait Ordering<ID> {
    fn dependencies(&self) -> &[ID];
}

pub struct Orderer<ID> {
    _marker: PhantomData<ID>,
}

impl<ID> Orderer<ID> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T, ID> Layer<T> for Orderer<ID>
where
    T: Ordering<ID> + Send + 'static,
    ID: Send + Sync + 'static,
{
    type Output = Option<T>;

    type Error = OrdererError;

    fn process(&self, _input: T) -> BoxFuture<'_, Result<(), Self::Error>> {
        Box::pin(async {
            // @TODO
            Ok(())
        })
    }

    fn next(&self) -> BoxFuture<'_, Result<Option<Self::Output>, Self::Error>> {
        Box::pin(async {
            // @TODO
            Ok(None)
        })
    }
}

#[derive(Debug, Error)]
pub enum OrdererError {}

#[cfg(test)]
mod tests {
    use futures_util::stream;
    use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
    use serde::{Deserialize, Serialize};

    use crate::{BufferedLayer, StreamExt};

    use super::{Orderer, Ordering};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct TestExtension {
        dependencies: Vec<Hash>,
    }

    impl Ordering<Hash> for Operation<TestExtension> {
        fn dependencies<'a>(&'a self) -> &'a [Hash] {
            match self.header.extensions {
                Some(ref extensions) => &extensions.dependencies,
                None => &[],
            }
        }
    }

    #[tokio::test]
    async fn out_of_order() {
        // Create two operations, one by Panda and one by Icebear. Panda's operation points at
        // Icebear's.
        let operation_panda = {
            let private_key = PrivateKey::new();
            let public_key = private_key.public_key();

            let body: Body = b"Hi, Icebear".to_vec().into();

            let mut header = Header {
                public_key,
                payload_size: body.size(),
                payload_hash: Some(body.hash()),
                extensions: Some(TestExtension {
                    dependencies: vec![],
                }),
                ..Default::default()
            };
            header.sign(&private_key);

            Operation {
                hash: header.hash(),
                header,
                body: Some(body),
            }
        };

        let operation_icebear = {
            let private_key = PrivateKey::new();
            let public_key = private_key.public_key();

            let body: Body = b"Hello, Pandasan!".to_vec().into();

            let mut header = Header {
                public_key,
                payload_size: body.size(),
                payload_hash: Some(body.hash()),
                extensions: Some(TestExtension {
                    dependencies: vec![operation_panda.hash],
                }),
                ..Default::default()
            };
            header.sign(&private_key);

            Operation {
                hash: header.hash(),
                header,
                body: Some(body),
            }
        };

        // Prepare processing pipeline for message ordering.
        let orderer = BufferedLayer::new(Orderer::<Hash>::new(), 16);

        // @TODO: Finish test.
        // Process Icebear's operation first. It will arrive "out of order".
        // Process Pandas's operation next. It will "free" Icebear's operation.
        let mut _stream =
            stream::iter(vec![operation_icebear, operation_panda]).process_with(orderer);
    }
}
