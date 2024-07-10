// SPDX-License-Identifier: AGPL-3.0-or-later

use futures::channel::mpsc;
use futures::SinkExt;
use std::ops::Deref;
use std::time::Duration;
use std::{cell::RefCell, rc::Rc};
use tokio::task;

use p2panda_core::{Extensions, Operation};

use crate::ingest::IngestResult;
use crate::{Context, Ingest, Layer, StreamEvent};

const SORT_INTERVAL_MS: u64 = 100;

#[derive(Clone, Default)]
enum Status {
    #[default]
    Idle,
    Running,
}

#[derive(Clone)]
pub struct Sorter<E: Extensions> {
    status: Rc<RefCell<Status>>,
    // @TODO: just using a Vec here for the queue, but we want a bounded
    // queue. We can use `crate::Queue` in the end.
    in_queue: Rc<RefCell<Vec<Operation<E>>>>,
    sender: Rc<RefCell<mpsc::Sender<Vec<Operation<E>>>>>,
}

impl<E: Extensions + 'static> Sorter<E> {
    pub fn run(&self) {
        // The sorter is already running, don't do anything
        if let Status::Running = self.status.borrow().deref() {
            return;
        }

        // Set status to running
        self.status.replace(Status::Running);

        // Get an `RC`` reference to the queue and sender
        let queue = self.in_queue.clone();
        let sender = self.sender.clone();

        // Spawn a task on the local thread pool
        task::spawn_local(async move {
            // We sort every `SORT_INTERVAL_MS` milliseconds
            let mut interval = tokio::time::interval(Duration::from_millis(SORT_INTERVAL_MS));
            loop {
                // Wait for the interval
                interval.tick().await;

                // Empty the queue and sort all operations which were in it.
                //
                // @TODO: here we want different queues per stream name maybe? At least some way
                // of grouping operations sensibly. We also need to get any other operations for
                // the streams we are sorting from the store.
                let mut operations: Vec<Operation<E>> = queue.replace(vec![]);
                operations.sort_by(|op_a, op_b| op_a.header.seq_num.cmp(&op_b.header.seq_num));

                // Send the sorted operations on the channel to be picked up by the SorterRoute
                let _ = sender.borrow_mut().send(operations).await;
            }
        });
    }

    pub fn queue_operation(&mut self, operation: Operation<E>) {
        self.in_queue.borrow_mut().push(operation);
    }
}

impl<E: Extensions> Sorter<E> {
    pub fn new(sender: mpsc::Sender<Vec<Operation<E>>>) -> Self {
        Sorter {
            status: Rc::new(RefCell::new(Status::default())),
            in_queue: Rc::new(RefCell::new(Vec::new())),
            sender: Rc::new(RefCell::new(sender)),
        }
    }
}

#[derive(Clone)]
pub struct SorterRoute<E: Extensions> {
    sorter: Sorter<E>,
    receiver: Rc<RefCell<mpsc::Receiver<Vec<Operation<E>>>>>,
}

impl<E: Extensions> SorterRoute<E> {
    pub fn new(receiver: mpsc::Receiver<Vec<Operation<E>>>, sorter: Sorter<E>) -> Self {
        SorterRoute {
            sorter,
            receiver: Rc::new(RefCell::new(receiver)),
        }
    }
}

pub struct SorterRouteService<M> {
    inner: Option<M>,
}

impl SorterRouteService<()> {
    pub fn new() -> Self {
        Self { inner: None }
    }
}

impl Ingest<()> for SorterRoute<()> {
    async fn ingest(&mut self, _context: Context, operation: &Operation<()>) -> IngestResult<()> {
        let running = match *self.sorter.status.borrow() {
            Status::Idle => false,
            Status::Running => true,
        };

        if !running {
            self.sorter.run()
        }

        // Check if operation will trigger re-sorting. 
        //
        // @TODO: This is just an arbitrary check for testing the sorter.
        // Eventually we will make a call to the store to see if the operation
        // we are ingesting triggers a re-sorting of the stream. 
        let trigger_sorting = operation.header.seq_num % 2 == 0;

        // If it doesn't just return now with the committed operation.
        if !trigger_sorting {
            return Ok(StreamEvent::Commit(operation.clone()));
        } else {
            // Put the operation on the sorter queue.
            self.sorter.queue_operation(operation.clone());
        }

        // Now check if any operation sorting was completed which we should pick up.
        match self.receiver.borrow_mut().try_next() {
            Ok(message) => match message {
                Some(operations) => return Ok(StreamEvent::Replay(operations)),
                None => return Ok(StreamEvent::None),
            },
            Err(_) => {
                return Ok(StreamEvent::None);
            }
        };
    }
}

impl<M, E: Extensions> Layer<M> for SorterRoute<E> {
    type Middleware = SorterRouteService<M>;

    fn layer(&self, inner: M) -> Self::Middleware {
        SorterRouteService { inner: Some(inner) }
    }
}
