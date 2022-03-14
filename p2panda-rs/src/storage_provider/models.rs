// SPDX-License-Identifier: AGPL-3.0-or-later
use std::fmt::Debug;

use crate::document::DocumentId;
use crate::entry::{decode_entry, Entry, EntrySigned, EntrySignedError, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::schema::SchemaId;

use super::conversions::{FromStorage, ToStorage};

pub trait AsEntry<T>: Sized + Send + Sync + FromStorage<T> + ToStorage<T> {
    type Error: Debug;

    fn entry_encoded(&self) -> EntrySigned;

    fn operation_encoded(&self) -> Option<OperationEncoded>;

    fn entry(&self) -> Entry {
        // Unwrapping optimistically for now...
        decode_entry(&self.entry_encoded(), self.operation_encoded().as_ref()).unwrap()
    }

    fn author(&self) -> Author {
        self.entry_encoded().author()
    }

    fn seq_num(&self) -> SeqNum {
        self.entry().seq_num().to_owned()
    }

    fn log_id(&self) -> LogId {
        self.entry().log_id().to_owned()
    }

    fn entry_hash(&self) -> Hash {
        self.entry_encoded().hash()
    }
}

pub trait AsLog<T>: Sized + Send + Sync + FromStorage<T> + ToStorage<T> {
    type Error: Debug;

    fn author(&self) -> Author;
    fn log_id(&self) -> LogId;
    fn document(&self) -> DocumentId;
    fn schema(&self) -> SchemaId;
}
