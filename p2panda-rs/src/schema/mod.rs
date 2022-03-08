// SPDX-License-Identifier: AGPL-3.0-or-later

//! Schemas describe the format of data used in operation fields.
mod error;
<<<<<<< HEAD
<<<<<<< HEAD
#[allow(clippy::module_inception)]
=======
>>>>>>> Introduce `Schema` struct (again...)
=======
#[allow(clippy::module_inception)]
>>>>>>> Make clipply a little happy
mod schema;
mod schema_id;
pub mod system;

pub use error::{SchemaError, SchemaIdError};
pub use schema_id::SchemaId;
