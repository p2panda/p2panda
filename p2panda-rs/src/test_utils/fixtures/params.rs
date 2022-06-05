// SPDX-License-Identifier: AGPL-3.0-or-later

/// General purpose fixtures which can be injected into rstest methods as parameters.
///
/// The fixtures can optionally be passed in with custom parameters which overrides the default
/// values.
use rstest::fixture;

use crate::document::{DocumentId, DocumentViewId};
use crate::operation::OperationId;
use crate::schema::SchemaId;
use crate::test_utils::constants::{DEFAULT_HASH, TEST_SCHEMA_ID};
use crate::test_utils::fixtures::*;

/// Fixture which injects the default schema id into a test method. Default value can be
/// overridden at testing time by passing in a custom schema id string.
#[fixture]
pub fn schema(#[default(TEST_SCHEMA_ID)] schema_id: &str) -> SchemaId {
    SchemaId::new(schema_id).unwrap()
}

/// Fixture which injects the default `DocumentId` into a test method. Default value can be overridden at
/// testing time by passing in a custom hash string.
#[fixture]
pub fn document_id(#[default(DEFAULT_HASH)] hash_str: &str) -> DocumentId {
    DocumentId::new(operation_id(hash_str))
}

/// Fixture which injects the default `DocumentViewId` into a test method. Default value can be
/// overridden at testing time by passing in a custom hash string vector.
#[fixture]
pub fn document_view_id(#[default(vec![DEFAULT_HASH])] hash_str_vec: Vec<&str>) -> DocumentViewId {
    let hashes: Vec<OperationId> = hash_str_vec
        .into_iter()
        .map(|hash| hash.parse::<OperationId>().unwrap())
        .collect();
    DocumentViewId::new(&hashes).unwrap()
}

/// Fixture which injects a random document id.
#[fixture]
pub fn random_document_id() -> DocumentId {
    DocumentId::new(random_hash().into())
}
