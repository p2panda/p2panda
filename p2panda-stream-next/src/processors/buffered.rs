// SPDX-License-Identifier: MIT OR Apache-2.0

use tokio::sync::mpsc;
use tokio::task::{self, JoinHandle};

use crate::processors::Processor;

/// Layer "driving" expensive async processors with an unbounded buffer, using a channel receiver
/// to input new items and forwarding processed items on a channel sender.
pub struct Buffer {
    handle: JoinHandle<()>,
}

pub type BufferSender<T> = mpsc::UnboundedSender<T>;

pub type BufferReceiver<P, T> =
    mpsc::UnboundedReceiver<Result<<P as Processor<T>>::Output, <P as Processor<T>>::Error>>;

impl Buffer {
    pub fn new<P, T>(processor: P) -> (Self, BufferSender<T>, BufferReceiver<P, T>)
    where
        P: Processor<T> + 'static,
        T: 'static,
    {
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<T>();
        let (output_tx, output_rx) = mpsc::unbounded_channel::<Result<P::Output, P::Error>>();

        let handle = task::spawn_local(async move {
            loop {
                tokio::select! {
                    input = input_rx.recv() => {
                        let Some(input) = input else {
                            break;
                        };

                        if let Err(err) = processor.process(input).await
                            && output_tx.send(Err(err)).is_err() {
                                break;
                            }
                    }

                    output = processor.next() => {
                        if output_tx.send(output).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        (Self { handle }, input_tx, output_rx)
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
