// SPDX-License-Identifier: AGPL-3.0-or-later

//! Create, encode and decode p2panda operations.
//!
//! Operations describe data mutations in the p2panda network. Authors send operations to create,
//! update or delete documents or collections of data.
mod decode;
mod encode;
mod encoded_operation;
mod error;
#[allow(clippy::module_inception)]
mod operation;
mod operation_action;
mod operation_fields;
mod operation_id;
mod operation_value;
mod operation_version;
mod raw_operation;
mod relation;
mod traits;
mod validate;
mod verified_operation;

pub use decode::decode_operation;
pub use encode::{encode_operation, encode_raw_operation};
pub use encoded_operation::EncodedOperation;
pub use error::{
    CBORError, DecodeOperationError, EncodedOperationError, OperationError, OperationFieldsError,
    RawOperationError, VerifiedOperationError,
};
pub use operation::{Operation, OperationBuilder};
pub use operation_action::OperationAction;
pub use operation_fields::OperationFields;
pub use operation_id::OperationId;
pub use operation_value::OperationValue;
pub use operation_version::OperationVersion;
pub use raw_operation::{RawFields, RawOperation, RawValue};
pub use relation::{PinnedRelation, PinnedRelationList, Relation, RelationList};
pub use traits::{AsOperation, AsVerifiedOperation};
pub use validate::verify_schema_and_convert;
pub use verified_operation::VerifiedOperation;
