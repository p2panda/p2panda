// SPDX-License-Identifier: MIT OR Apache-2.0

use thiserror::Error;

use crate::processors::Processor;

pub struct ComposedProcessors<P1, P2> {
    pub first: P1,
    pub second: P2,
}

impl<P1, P2, T> Processor<T> for ComposedProcessors<P1, P2>
where
    P1: Processor<T>,
    P2: Processor<P1::Output>,
{
    type Output = P2::Output;

    type Error = ComposedError<P1::Error, P2::Error>;

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        self.first
            .process(input)
            .await
            .map_err(ComposedError::First)?;
        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        loop {
            tokio::select! {
                intermediate = self.first.next() => {
                    match intermediate {
                        Ok(intermediate) => {
                            self.second
                                .process(intermediate)
                                .await
                                .map_err(ComposedError::Second)?;

                            // Yield to prevent runtime starvation.
                            tokio::task::yield_now().await;
                        },
                        Err(err) => {
                            return Err(ComposedError::First(err));
                        },
                    }
                }

                output = self.second.next() => {
                    return output.map_err(ComposedError::Second);
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum ComposedError<P1, P2> {
    #[error("{0}")]
    First(P1),

    #[error("{0}")]
    Second(P2),
}
