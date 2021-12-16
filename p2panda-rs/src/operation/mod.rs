// SPDX-License-Identifier: AGPL-3.0-or-later

//! Create, encode and decode p2pandan operations.
//!
//! Operations describe data mutations in the p2panda network. Authors send operations to create,
//! update or delete instances or collections of data.
mod error;
#[allow(clippy::module_inception)]
mod operation;
mod operation_encoded;

pub use error::{OperationEncodedError, OperationError, OperationFieldsError};
pub use operation::{
    AsOperation, Operation, OperationAction, OperationFields, OperationValue, OperationVersion,
};
pub use operation_encoded::OperationEncoded;
