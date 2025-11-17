// SPDX-License-Identifier: MIT OR Apache-2.0

pub mod ephemeral;
pub mod eventually_consistent;

use thiserror::Error;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::mpsc::error::SendError;

use crate::TopicId;

#[derive(Debug, Error)]
pub enum StreamError<T> {
    #[error(transparent)]
    Send(#[from] SendError<T>),

    #[error(transparent)]
    Recv(#[from] RecvError),

    #[error(transparent)]
    TryRecv(#[from] TryRecvError),

    #[error("failed to create stream for topic {0:?} due to system error")]
    Create(TopicId),

    #[error("failed to subscribe to topic {0:?} due to system error")]
    Subscribe(TopicId),

    #[error("failed to close stream for topic {0:?}")]
    Close(TopicId),

    #[error("no stream exists for the given topic")]
    StreamNotFound,

    #[error("failed to publish to topic {0:?} due to system error")]
    Publish(TopicId),
}
