// SPDX-License-Identifier: AGPL-3.0-or-later

mod body;
pub mod error;
mod header;
#[allow(clippy::module_inception)]
mod operation;
mod operation_action;
mod operation_fields;
mod operation_id;
mod operation_value;
mod operation_version;
mod relation;
pub mod traits;
pub mod validate;

pub use operation::Operation;
pub use operation_action::OperationAction;
pub use operation_fields::OperationFields;
pub use operation_id::OperationId;
pub use operation_value::OperationValue;
pub use operation_version::OperationVersion;
pub use relation::{PinnedRelation, PinnedRelationList, Relation, RelationList};
