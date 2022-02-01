// SPDX-License-Identifier: AGPL-3.0-or-later

//! Create, encode and decode p2panda operations.
//!
//! Operations describe data mutations in the p2panda network. Authors send operations to create,
//! update or delete documents or collections of data.
mod error;
#[allow(clippy::module_inception)]
mod operation;
mod operation_encoded;
mod operation_signed;

pub use error::{
    OperationEncodedError, OperationError, OperationFieldsError, OperationSignedError,
};
pub use operation::{
    AsOperation, Operation, OperationAction, OperationFields, OperationValue, OperationVersion,
};
pub use operation_encoded::OperationEncoded;
pub use operation_signed::OperationSigned;
