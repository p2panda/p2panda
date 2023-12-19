// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::schema::SchemaId;
use crate::operation::body::plain::PlainFields;

/// Trait representing an "operation-like" struct which contains data fields that can be checked
/// against a schema.
pub trait Schematic {
    /// Returns the schema id.
    fn schema_id(&self) -> &SchemaId;

    /// Returns the fields holding the data.
    fn plain_fields(&self) -> Option<PlainFields>;
}
