// SPDX-License-Identifier: AGPL-3.0-or-later

use once_cell::sync::Lazy;

use crate::schema::error::SchemaIdError;
use crate::schema::{FieldType, Schema, SchemaDescription, SchemaFields, SchemaId};

const DESCRIPTION: &str = "Representation of the (partial) binary data of a file.";

pub static BLOB_PIECE_V1: Lazy<Schema> = Lazy::new(|| {
    let fields = SchemaFields::new(&[
        ("data", FieldType::String),
    ])
    // Unwrap as we know the fields are valid.
    .unwrap();

    // We can unwrap here as we know the schema definition is valid.
    let description = SchemaDescription::new(DESCRIPTION).unwrap();

    Schema {
        id: SchemaId::BlobPiece(1),
        description,
        fields,
    }
});

/// Returns the `schema_definition` system schema with a given version.
pub fn get_blob_piece(version: u8) -> Result<&'static Schema, SchemaIdError> {
    match version {
        1 => Ok(&BLOB_PIECE_V1),
        _ => Err(SchemaIdError::UnknownSystemSchema(
            SchemaId::BlobPiece(version).to_string(),
        )),
    }
}
