// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod decode;
pub mod encode;
mod encoded_operation;
pub mod error;
#[allow(clippy::module_inception)]
mod operation;
mod operation_action;
mod operation_fields;
mod operation_id;
mod operation_value;
mod operation_version;
pub mod plain;
mod relation;
pub mod validate;

pub use encoded_operation::EncodedOperation;
pub use operation::{Body, BodyBuilder};
pub use operation_action::OperationAction;
pub use operation_fields::OperationFields;
pub use operation_id::OperationId;
pub use operation_value::OperationValue;
pub use operation_version::OperationVersion;
pub use relation::{PinnedRelation, PinnedRelationList, Relation, RelationList};