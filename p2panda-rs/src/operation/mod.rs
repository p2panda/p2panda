// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod body;
pub mod error;
pub mod header;
#[allow(clippy::module_inception)]
mod operation;
mod operation_action;
mod operation_fields;
mod operation_id;
mod operation_value;
mod operation_version;
mod relation;
pub mod traits;
pub mod validation;

pub use operation::{Operation, OperationBuilder};
pub use operation_action::OperationAction;
pub use operation_fields::OperationFields;
pub use operation_id::OperationId;
pub use operation_value::OperationValue;
pub use operation_version::OperationVersion;
pub use relation::{PinnedRelation, PinnedRelationList, Relation, RelationList};
