// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::document::{DocumentId, DocumentViewId};
use crate::operation::OperationId;
use crate::test_utils::constants::HASH;
use crate::test_utils::fixtures::random_hash;

/// Fixture which injects the default `DocumentId` into a test method.
///
/// Default value can be overridden at testing time by passing in a custom hash string.
#[fixture]
pub fn document_id(#[default(HASH)] hash_str: &str) -> DocumentId {
    hash_str.parse().unwrap()
}

/// Fixture which injects the default `DocumentViewId` into a test method.
///
/// Default value can be overridden at testing time by passing in a custom vector of hash strings.
#[fixture]
pub fn document_view_id(#[default(vec![HASH])] hash_str_vec: Vec<&str>) -> DocumentViewId {
    let hashes: Vec<OperationId> = hash_str_vec
        .into_iter()
        .map(|hash| hash.parse::<OperationId>().unwrap())
        .collect();
    DocumentViewId::new(&hashes).unwrap()
}

/// Fixture which injects a random `DocumentId` into a test method.
#[fixture]
pub fn random_document_id() -> DocumentId {
    random_hash().into()
}

/// Fixture which injects a random `DocumentViewId` into a test method.
#[fixture]
pub fn random_document_view_id() -> DocumentViewId {
    random_hash().into()
}
