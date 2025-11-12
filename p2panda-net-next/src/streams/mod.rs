pub mod ephemeral;
pub mod eventually_consistent;

use thiserror::Error;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::error::SendError;

#[derive(Debug, Error)]
pub enum StreamError<T> {
    #[error(transparent)]
    Send(#[from] SendError<T>),

    #[error(transparent)]
    Recv(#[from] RecvError),

    #[error("actor {0} failed to process request")]
    Actor(String),

    #[error("failed to call {0} actor; it may be in the process of restarting")]
    ActorNotFound(String),

    #[error("no stream exists for the given topic")]
    StreamNotFound,
}
