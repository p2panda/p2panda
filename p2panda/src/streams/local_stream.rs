// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::operation::Operation;
use crate::spaces::types::SpacesEvent;

pub enum LocalStreamDestination {
    Delivery(Operation),
    Processing(Operation, Vec<SpacesEvent>),
}

impl LocalStreamDestination {
    pub fn processing(operation: Operation) -> Self {
        Self::Processing(operation, vec![])
    }

    pub fn processing_enriched(operation: Operation, spaces_events: Vec<SpacesEvent>) -> Self {
        Self::Processing(operation, spaces_events)
    }
}

#[cfg(test)]
impl LocalStreamDestination {
    pub fn operation(&self) -> &Operation {
        match self {
            Self::Delivery(operation) => operation,
            Self::Processing(operation, _) => operation,
        }
    }
}
