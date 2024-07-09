// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashSet, VecDeque};

use p2panda_core::{Extensions, Hash, Operation};

#[derive(Clone)]
pub struct Queue<E>
where
    E: Extensions,
{
    queue: VecDeque<Operation<E>>,
    hashes: HashSet<Hash>,
    buffer: usize,
}

impl<E> Queue<E>
where
    E: Extensions,
{
    pub fn new(buffer: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(buffer),
            hashes: HashSet::with_capacity(buffer),
            buffer,
        }
    }

    pub fn is_full(&self) -> bool {
        self.queue.len() >= self.buffer
    }

    pub fn push(&mut self, operation: Operation<E>) -> Option<Operation<E>> {
        if self.is_full() {
            Some(operation)
        } else {
            self.hashes.insert(operation.hash);
            self.queue.push_back(operation);
            None
        }
    }

    pub fn contains(&self, operation: &Operation<E>) -> bool {
        self.hashes.contains(&operation.hash)
    }

    pub fn front(&self) -> Option<&Operation<E>> {
        self.queue.front()
    }

    pub fn pop(&mut self) -> Option<Operation<E>> {
        match self.queue.pop_front() {
            Some(operation) => {
                self.hashes.remove(&operation.hash);
                Some(operation)
            }
            None => None,
        }
    }
}
