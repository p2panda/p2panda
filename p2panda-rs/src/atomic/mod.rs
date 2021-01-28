use crate::error::Result;

mod author;
mod entry;
mod entry_encoded;
mod hash;
mod log_id;
mod message;
mod message_encoded;
mod seq_num;

pub use author::Author;
pub use entry::Entry;
pub use entry_encoded::EntryEncoded;
pub use hash::Hash;
pub use log_id::LogId;
pub use message::{Message, MessageAction, MessageFields, MessageValue, MessageVersion};
pub use message_encoded::MessageEncoded;
pub use seq_num::SeqNum;

pub mod errors {
    pub use super::author::AuthorError;
    pub use super::hash::HashError;
    pub use super::message::{MessageError, MessageFieldsError};
    pub use super::message_encoded::MessageEncodedError;
    pub use super::seq_num::SeqNumError;
}

pub trait Validation {
    /// Validates atomic data types instance.
    fn validate(&self) -> Result<()>;
}
