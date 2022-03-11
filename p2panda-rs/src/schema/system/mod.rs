// SPDX-License-Identifier: AGPL-3.0-or-later

//! System schemas are p2panda's built-in schema type.
//!
//! They are defined as part of the p2panda specification and may differ from application schemas
//! in how they are materialised.
mod error;
mod schema_views;

pub use error::SystemSchemaError;
pub use schema_views::{SchemaFieldView, SchemaView};
