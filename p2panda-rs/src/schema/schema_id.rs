// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::fmt::Display;
use std::str::FromStr;

use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use yasmf_hash::MAX_YAMF_HASH_SIZE;

use crate::document::DocumentViewId;
use crate::operation::OperationId;
use crate::schema::error::SchemaIdError;
use crate::schema::SchemaName;
use crate::Human;

/// Spelling of _schema definition_ schema
pub(super) const SCHEMA_DEFINITION_NAME: &str = "schema_definition";

/// Spelling of _schema field definition_ schema
pub(super) const SCHEMA_FIELD_DEFINITION_NAME: &str = "schema_field_definition";

/// Spelling of _blob_ schema
pub(super) const BLOB_NAME: &str = "blob";

/// Spelling of _blob piece_ schema
pub(super) const BLOB_PIECE_NAME: &str = "blob_piece";

/// Represent a schema's version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaVersion {
    /// An application schema's version contains its document view id.
    Application(DocumentViewId),

    /// A system schema's version contains an integer version number.
    System(u8),
}

/// Identifies the schema of an [`Operation`][`crate::operation::Operation`] or
/// [`Document`][`crate::document::Document`].
///
/// Every schema id has a name and version.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SchemaId {
    /// An application schema.
    Application(SchemaName, DocumentViewId),

    /// A schema definition.
    SchemaDefinition(u8),

    /// A schema definition field.
    SchemaFieldDefinition(u8),

    /// A blob.
    Blob(u8),

    /// A blob piece.
    BlobPiece(u8),
}

impl SchemaId {
    /// Instantiate a new `SchemaId`.
    ///
    /// ```
    /// # use p2panda_rs::schema::SchemaId;
    /// let system_schema = SchemaId::new("schema_definition_v1");
    /// assert!(system_schema.is_ok());
    ///
    /// let application_schema = SchemaId::new(
    ///     "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
    /// );
    /// assert!(application_schema.is_ok());
    /// ```
    pub fn new(id: &str) -> Result<Self, SchemaIdError> {
        // Retrieve the rightmost section separated by an underscore and check whether it follows
        // the version format of system schemas (e.g. `..._v1`).
        let rightmost_section = id
            .rsplit_once('_')
            .ok_or_else(|| {
                SchemaIdError::MalformedSchemaId(
                    id.to_string(),
                    "doesn't contain an underscore".to_string(),
                )
            })?
            .1;

        let is_system_schema =
            rightmost_section.starts_with('v') && rightmost_section.len() < MAX_YAMF_HASH_SIZE * 2;

        match is_system_schema {
            true => Self::parse_system_schema_str(id),
            false => Self::parse_application_schema_str(id),
        }
    }

    /// Returns a `SchemaId` given an application schema's name and view id.
    pub fn new_application(name: &SchemaName, view_id: &DocumentViewId) -> Self {
        Self::Application(name.to_owned(), view_id.clone())
    }

    /// Access the schema name.
    pub fn name(&self) -> SchemaName {
        match self {
            SchemaId::Application(name, _) => name.to_owned(),
            // We unwrap here as we know system schema names are valid names.
            SchemaId::Blob(_) => SchemaName::new(BLOB_NAME).unwrap(),
            SchemaId::BlobPiece(_) => SchemaName::new(BLOB_PIECE_NAME).unwrap(),
            SchemaId::SchemaDefinition(_) => SchemaName::new(SCHEMA_DEFINITION_NAME).unwrap(),
            SchemaId::SchemaFieldDefinition(_) => {
                SchemaName::new(SCHEMA_FIELD_DEFINITION_NAME).unwrap()
            }
        }
    }

    /// Access the schema version.
    pub fn version(&self) -> SchemaVersion {
        match self {
            SchemaId::Application(_, view_id) => SchemaVersion::Application(view_id.clone()),
            SchemaId::Blob(version) => SchemaVersion::System(*version),
            SchemaId::BlobPiece(version) => SchemaVersion::System(*version),
            SchemaId::SchemaDefinition(version) => SchemaVersion::System(*version),
            SchemaId::SchemaFieldDefinition(version) => SchemaVersion::System(*version),
        }
    }
}

impl SchemaId {
    /// Read a system schema id from a string.
    fn parse_system_schema_str(id_str: &str) -> Result<Self, SchemaIdError> {
        let (name, version_str) = id_str.rsplit_once('_').unwrap();

        let version = version_str[1..].parse::<u8>().map_err(|_| {
            SchemaIdError::MalformedSchemaId(
                id_str.to_string(),
                "couldn't parse system schema version".to_string(),
            )
        })?;

        match name {
            SCHEMA_DEFINITION_NAME => Ok(Self::SchemaDefinition(version)),
            SCHEMA_FIELD_DEFINITION_NAME => Ok(Self::SchemaFieldDefinition(version)),
            BLOB_NAME => Ok(Self::Blob(version)),
            BLOB_PIECE_NAME => Ok(Self::BlobPiece(version)),
            _ => Err(SchemaIdError::UnknownSystemSchema(name.to_string())),
        }
    }

    /// Read an application schema id from a string.
    ///
    /// Parses the schema id by iteratively splitting sections from the right at `_` until the
    /// remainder is shorter than an operation id. Each section is parsed as an operation id
    /// and the last (leftmost) section is parsed as the schema's name.
    fn parse_application_schema_str(id_str: &str) -> Result<Self, SchemaIdError> {
        let mut operation_ids = vec![];
        let mut remainder = id_str;

        while let Some((left, right)) = remainder.rsplit_once('_') {
            let operation_id: OperationId = right.parse()?;
            operation_ids.push(operation_id);

            // If the remainder is no longer than an entry hash we assume that it's the schema
            // name. By breaking here we allow the schema name to contain underscores as well.
            remainder = left;
            if remainder.len() < MAX_YAMF_HASH_SIZE * 2 {
                break;
            }
        }

        // Since we've built the array from the back, we have to reverse it again to get the
        // original order
        operation_ids.reverse();

        // Validate if the name is given and correct
        if remainder.is_empty() {
            return Err(SchemaIdError::MissingApplicationSchemaName(
                id_str.to_string(),
            ));
        }

        let name = match remainder.parse() {
            Ok(name) => Ok(name),
            Err(_) => Err(SchemaIdError::MalformedSchemaId(
                id_str.to_string(),
                "name contains too many or invalid characters".to_string(),
            )),
        }?;

        Ok(SchemaId::Application(
            name,
            DocumentViewId::from_untrusted(operation_ids)?,
        ))
    }
}

impl Display for SchemaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaId::Application(name, view_id) => {
                write!(f, "{}", name)?;

                view_id
                    .iter()
                    .try_for_each(|op_id| write!(f, "_{}", op_id.as_str()))?;

                Ok(())
            }
            SchemaId::Blob(version) => {
                write!(f, "{}_v{}", BLOB_NAME, version)
            }
            SchemaId::BlobPiece(version) => {
                write!(f, "{}_v{}", BLOB_PIECE_NAME, version)
            }
            SchemaId::SchemaDefinition(version) => {
                write!(f, "{}_v{}", SCHEMA_DEFINITION_NAME, version)
            }
            SchemaId::SchemaFieldDefinition(version) => {
                write!(f, "{}_v{}", SCHEMA_FIELD_DEFINITION_NAME, version)
            }
        }
    }
}

impl Human for SchemaId {
    fn display(&self) -> String {
        match self {
            SchemaId::Application(name, view_id) => format!("{} {}", name, view_id.display()),
            system_schema => format!("{}", system_schema),
        }
    }
}

impl FromStr for SchemaId {
    type Err = SchemaIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for SchemaId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SchemaId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SchemaIdVisitor;

        impl<'de> Visitor<'de> for SchemaIdVisitor {
            type Value = SchemaId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("schema id as string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                SchemaId::new(value).map_err(|err| serde::de::Error::custom(err.to_string()))
            }
        }

        deserializer.deserialize_any(SchemaIdVisitor)
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use crate::schema::SchemaName;
    use crate::test_utils::constants::SCHEMA_ID;
    use crate::test_utils::fixtures::schema_id;
    use crate::Human;

    use super::SchemaId;

    #[rstest]
    #[case(
        SchemaId::new(SCHEMA_ID).unwrap(),
        "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
    )]
    #[case(SchemaId::SchemaDefinition(1), "schema_definition_v1")]
    #[case(SchemaId::SchemaFieldDefinition(1), "schema_field_definition_v1")]
    #[case(SchemaId::Blob(1), "blob_v1")]
    #[case(SchemaId::BlobPiece(1), "blob_piece_v1")]
    fn serialize(#[case] schema_id: SchemaId, #[case] expected_schema_id_string: &str) {
        let mut cbor_bytes = Vec::new();
        let mut expected_cbor_bytes = Vec::new();

        ciborium::ser::into_writer(&schema_id, &mut cbor_bytes).unwrap();
        ciborium::ser::into_writer(expected_schema_id_string, &mut expected_cbor_bytes).unwrap();

        assert_eq!(cbor_bytes, expected_cbor_bytes);
    }

    #[rstest]
    #[case(
        SchemaId::new_application(&SchemaName::new("venue").unwrap(), &"0020ce6f2c08e56836d6c3eb4080d6cc948dba138cba328c28059f45ebe459901771".parse().unwrap()
        ),
        "venue_0020ce6f2c08e56836d6c3eb4080d6cc948dba138cba328c28059f45ebe459901771"
    )]
    #[case(SchemaId::SchemaDefinition(1), "schema_definition_v1")]
    #[case(SchemaId::SchemaFieldDefinition(1), "schema_field_definition_v1")]
    #[case(SchemaId::Blob(1), "blob_v1")]
    #[case(SchemaId::BlobPiece(1), "blob_piece_v1")]
    fn deserialize(#[case] schema_id: SchemaId, #[case] expected_schema_id_string: &str) {
        let parsed_app_schema: SchemaId = expected_schema_id_string.parse().unwrap();
        assert_eq!(schema_id, parsed_app_schema);
    }

    // Not a hash at all
    #[rstest]
    #[case(
        "This is not a hash",
        "malformed schema id `This is not a hash`: doesn't contain an underscore"
    )]
    // Only an operation id, could be interpreted as document view id but still missing the name
    #[case(
        "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
        "malformed schema id `0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b`: doesn't contain an underscore"
    )]
    // Only the name is missing now
    #[case(
        "_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
        "application schema id is missing a name: _0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c\
        7b9ab46293111c48fc78b"
    )]
    // Name contains invalid characters
    #[case(
        "abc2%_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
        "malformed schema id `abc2%_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b`: name contains too many or invalid characters"
    )]
    // This name is too long, parser will fail trying to read its last section as an operation id
    #[case(
        "this_name_is_way_too_long_it_cant_be_good_to_have_such_a_long_name_to_be_honest_0020c65\
        567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
        "encountered invalid hash while parsing application schema id: invalid hex encoding in \
        hash string"
    )]
    // This hash is malformed
    #[case(
        "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc7",
        "encountered invalid hash while parsing application schema id: invalid hash length 33 \
        bytes, expected 34 bytes"
    )]
    // this looks like a system schema, but it is not
    #[case(
        "unknown_system_schema_name_v1",
        "unsupported system schema: unknown_system_schema_name"
    )]
    // malformed system schema version number
    #[case(
        "schema_definition_v1.5",
        "malformed schema id `schema_definition_v1.5`: couldn't parse system schema version"
    )]
    fn invalid_deserialization(#[case] schema_id_str: &str, #[case] expected_err: &str) {
        assert_eq!(
            format!("{}", schema_id_str.parse::<SchemaId>().unwrap_err()),
            expected_err
        );
    }

    #[test]
    fn new_schema_type() {
        let appl_schema = SchemaId::new(
            "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
        )
        .unwrap();
        assert_eq!(
            appl_schema,
            SchemaId::new(
                "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
            )
            .unwrap()
        );

        assert_eq!(
            format!("{}", appl_schema),
            "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
        );

        let schema = SchemaId::new("schema_definition_v50").unwrap();
        assert_eq!(schema, SchemaId::SchemaDefinition(50));
        assert_eq!(format!("{}", schema), "schema_definition_v50");

        let schema_field = SchemaId::new("schema_field_definition_v1").unwrap();
        assert_eq!(schema_field, SchemaId::SchemaFieldDefinition(1));
        assert_eq!(format!("{}", schema_field), "schema_field_definition_v1");
    }

    #[test]
    fn from_str() {
        let schema: SchemaId = "schema_definition_v1".parse().unwrap();
        assert_eq!(schema, SchemaId::SchemaDefinition(1));
    }

    #[rstest]
    fn string_representation(schema_id: SchemaId) {
        assert_eq!(
            schema_id.to_string(),
            "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
        );
        assert_eq!(
            format!("{}", schema_id),
            "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
        );
        assert_eq!(format!("{}", SchemaId::Blob(1)), "blob_v1");
        assert_eq!(format!("{}", SchemaId::BlobPiece(1)), "blob_piece_v1");
        assert_eq!(
            format!("{}", SchemaId::SchemaDefinition(1)),
            "schema_definition_v1"
        );
        assert_eq!(
            format!("{}", SchemaId::SchemaFieldDefinition(1)),
            "schema_field_definition_v1"
        );
    }

    #[rstest]
    fn short_representation(schema_id: SchemaId) {
        assert_eq!(schema_id.display(), "venue 8fc78b");
        assert_eq!(
            SchemaId::SchemaDefinition(1).display(),
            "schema_definition_v1"
        );
    }
}
