// SPDX-License-Identifier: AGPL-3.0-or-later

//! Intermediary operation type which was not checked against a schema yet.
//!
//! The `PlainOperation` serves as the binding data type which is an already decoded operation
//! which has not been checked against a `Schema` instance yet. This allows us to a) already read
//! header information from it, like the schema id, operation action or -version b) efficiently
//! deserialize even when we don't know the schema.
mod plain_fields;
mod plain_operation;
mod plain_value;

pub use plain_fields::PlainFields;
pub use plain_operation::PlainOperation;
pub use plain_value::PlainValue;
