//! Create, encode and decode p2panda messages.
//!
//! Messages describe data mutations in the p2panda network. Authors send messages to create,
//! update or delete instances or collections of data.
mod error;
mod message;
mod message_encoded;

pub use error::{MessageEncodedError, MessageError, MessageFieldsError};
pub use message::{Message, MessageAction, MessageFields, MessageValue};
pub use message_encoded::MessageEncoded;
