// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use thiserror::Error;

use crate::Layer;

pub trait Ordering<ID> {
    fn dependencies<'a>(&'a self) -> &'a [ID];
}

pub struct Orderer<ID> {
    _marker: PhantomData<ID>,
}

impl<ID> Orderer<ID> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T, ID> Layer<T> for Orderer<ID>
where
    T: Ordering<ID>,
{
    type Output = Option<T>;

    type Error = OrdererError;

    async fn process(&self, _input: T) -> Result<Self::Output, Self::Error> {
        // @TODO
        Ok(None)
    }

    // @TODO: We need this.
    // async fn next(&mut self) -> Option<Self::Output> {
    //     None
    // }
}

#[derive(Debug, Error)]
pub enum OrdererError {}

#[cfg(test)]
mod tests {
    use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
    use serde::{Deserialize, Serialize};

    use crate::Layer;

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
        let layer = Orderer::<Hash>::new();

        // Process Icebear's operation first. It will arrive "out of order".
        layer.process(operation_icebear).await.unwrap();

        // Process Pandas's operation next. It will "free" Icebear's operation.
        layer.process(operation_panda).await.unwrap();
    }
}
