// SPDX-License-Identifier: AGPL-3.0-or-later

/// General purpose fixtures which can be injected into rstest methods as parameters.
///
/// The fixtures can optionally be passed in with custom parameters which overrides the default
/// values.
use std::convert::TryFrom;

use rstest::fixture;

use crate::document::{DocumentId, DocumentViewId};
use crate::identity::{Author, KeyPair};
use crate::operation::OperationId;
use crate::schema::SchemaId;
use crate::test_utils::constants::{DEFAULT_HASH, DEFAULT_PRIVATE_KEY, TEST_SCHEMA_ID};
use crate::test_utils::fixtures::*;
use crate::test_utils::utils;

/// Fixture which injects the default private key string into a test method.
#[fixture]
pub fn private_key() -> String {
    DEFAULT_PRIVATE_KEY.into()
}

/// Fixture which injects the default author into a test method.
#[fixture]
pub fn public_key() -> Author {
    let key_pair = KeyPair::from_private_key_str(DEFAULT_PRIVATE_KEY).unwrap();
    Author::try_from(key_pair.public_key().to_owned()).unwrap()
}

/// Fixture which injects the default KeyPair into a test method. Default value can be overridden
/// at testing time by passing in a custom private key string.
#[fixture]
pub fn key_pair(#[default(DEFAULT_PRIVATE_KEY.into())] private_key: String) -> KeyPair {
    utils::keypair_from_private(private_key)
}

/// Fixture which injects a random KeyPair into a test method.
#[fixture]
pub fn random_key_pair() -> KeyPair {
    utils::new_key_pair()
}

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
