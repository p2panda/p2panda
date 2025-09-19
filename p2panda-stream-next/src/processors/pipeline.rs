// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use crate::processors::{Processor, chained::ChainedProcessors};

#[derive(Default)]
pub struct PipelineBuilder<T> {
    _marker: PhantomData<T>,
}

impl<T> PipelineBuilder<T> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    pub fn layer<P>(self, processor: P) -> LayeredBuilder<P, T> {
        LayeredBuilder {
            processor,
            _marker: PhantomData,
        }
    }
}

pub struct LayeredBuilder<P, T> {
    processor: P,
    _marker: PhantomData<T>,
}

impl<P, T> LayeredBuilder<P, T> {
    pub fn layer<P2>(self, processor: P2) -> LayeredBuilder<ChainedProcessors<P, P2>, T>
    where
        P: Processor<T>,
        P2: Processor<P::Output>,
    {
        LayeredBuilder {
            processor: ChainedProcessors {
                first: self.processor,
                second: processor,
            },
            _marker: PhantomData,
        }
    }

    pub fn build(self) -> Pipeline<P> {
        Pipeline {
            processor: self.processor,
        }
    }
}

pub struct Pipeline<P> {
    processor: P,
}

impl<P, T> Processor<T> for Pipeline<P>
where
    P: Processor<T>,
{
    type Output = P::Output;

    type Error = P::Error;

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        self.processor.process(input).await
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        self.processor.next().await
    }
}
