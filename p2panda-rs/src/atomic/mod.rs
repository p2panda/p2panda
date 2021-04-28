mod author;
mod entry;
mod entry_signed;
mod hash;
mod log_id;
mod message;
mod message_encoded;
mod seq_num;

pub use author::Author;
pub use entry::Entry;
pub use entry_signed::EntrySigned;
pub use hash::Hash;
pub use hash::Blake2BArrayVec;
pub use log_id::LogId;
pub use message::{Message, MessageAction, MessageFields, MessageValue, MessageVersion};
pub use message_encoded::MessageEncoded;
pub use seq_num::SeqNum;

/// Custom error types of atomic structs.
pub mod error {
    pub use super::author::AuthorError;
    pub use super::entry::EntryError;
    pub use super::entry_signed::EntrySignedError;
    pub use super::hash::HashError;
    pub use super::message::{MessageError, MessageFieldsError};
    pub use super::message_encoded::MessageEncodedError;
    pub use super::seq_num::SeqNumError;
}

/// Trait used by atomic structs to validate arguments.
pub trait Validation {
    /// Validation error type.
    type Error;

    /// Validates atomic data types instance.
    fn validate(&self) -> Result<(), Self::Error>;
}
