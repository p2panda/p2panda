// SPDX-License-Identifier: MIT OR Apache-2.0

use async_channel::{Receiver, Sender};
use thiserror::Error;
use tokio::task::{self, JoinHandle};

use crate::processors::Processor;

/// Processor "driving" another processor with a sized buffer, using an input stream to input new
/// items and yielding processed items as a `Stream` implementation.
///
/// ## Errors
///
/// The processor silently fails on internal errors (channels closed, etc.), but forwards any
/// potential failures occuring in the inner processor (both when calling `process` or `next`). Any
/// higher-level logic can now reason about if that error should be forwarded or if the wrapper
/// (and thus the inner processor itself) needs to be stopped.
pub struct BufferedProcessor<P, T>
where
    P: Processor<T>,
{
    input_tx: Sender<T>,
    output_rx: Receiver<Result<P::Output, P::Error>>,
    handle: JoinHandle<()>,
}

impl<P, T> BufferedProcessor<P, T>
where
    P: Processor<T> + 'static,
    T: 'static,
{
    pub fn new(processor: P, buffer_size: usize) -> Self {
        let (input_tx, input_rx) = async_channel::bounded::<T>(buffer_size);
        let (output_tx, output_rx) =
            async_channel::bounded::<Result<P::Output, P::Error>>(buffer_size);

        let handle = task::spawn_local(async move {
            loop {
                tokio::select! {
                    input = input_rx.recv() => {
                        let Ok(input) = input else {
                            break;
                        };

                        if let Err(err) = processor.process(input).await
                            && output_tx.send(Err(err)).await.is_err() {
                                break;
                            }
                    }

                    output = processor.next() => {
                        if output_tx.send(output).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Self {
            input_tx,
            output_rx,
            handle,
        }
    }
}

impl<P, T> Drop for BufferedProcessor<P, T>
where
    P: Processor<T>,
{
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl<P, T> Processor<T> for BufferedProcessor<P, T>
where
    P: Processor<T>,
{
    type Output = Result<P::Output, P::Error>;

    type Error = BufferedProcessorError;

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        // Ignore channel errors as this just indicates that the processor was shut down from the
        // outside.
        let _ = self.input_tx.send(input).await;

        // Do not forward any errors which might occur from calling "process". Users of this "meta
        // processor" around the inner processor will eventually receive it via "next". This is due
        // to the stream design where all errors are "merged" into one output result.
        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        match self.output_rx.recv().await {
            Ok(output) => Ok(output),
            Err(_) => Err(BufferedProcessorError::Terminated),
        }
    }
}

#[derive(Debug, Error)]
pub enum BufferedProcessorError {
    #[error("processor was terminated")]
    Terminated,
}
