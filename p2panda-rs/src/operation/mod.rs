// SPDX-License-Identifier: AGPL-3.0-or-later

//! Create, encode and decode p2panda operations.
//!
//! Operations describe data mutations in the p2panda network. Authors send operations to create,
//! update or delete documents or collections of data.
mod decode;
mod encode;
mod error;
#[allow(clippy::module_inception)]
mod operation;
mod operation_encoded;
mod operation_fields;
mod operation_id;
mod operation_value;
mod raw_operation;
mod relation;
mod traits;
mod verified_operation;

pub use error::{
    OperationEncodedError, OperationError, OperationFieldsError, VerifiedOperationError,
};
pub use operation::{Operation, OperationAction, OperationVersion};
pub use operation_encoded::OperationEncoded;
pub use operation_fields::OperationFields;
pub use operation_id::OperationId;
pub use operation_value::OperationValue;
pub use raw_operation::RawOperation;
pub use relation::{PinnedRelation, PinnedRelationList, Relation, RelationList};
pub use traits::{AsOperation, AsVerifiedOperation};
pub use verified_operation::VerifiedOperation;
