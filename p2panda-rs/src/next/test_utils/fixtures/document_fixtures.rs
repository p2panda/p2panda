// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::next::document::{DocumentId, DocumentViewId};
use crate::next::operation::OperationId;
use crate::next::test_utils::constants::HASH;
use crate::next::test_utils::fixtures::random_hash;

/// Returns constant document id.
#[fixture]
pub fn document_id(#[default(HASH)] hash_str: &str) -> DocumentId {
    hash_str.parse().unwrap()
}

/// Returns constant document view id.
#[fixture]
pub fn document_view_id(#[default(vec![HASH])] operation_id_str_vec: Vec<&str>) -> DocumentViewId {
    let operation_ids: Vec<OperationId> = operation_id_str_vec
        .into_iter()
        .map(|hash| hash.parse::<OperationId>().unwrap())
        .collect();

    DocumentViewId::new(&operation_ids)
}

/// Generates random document id.
#[fixture]
pub fn random_document_id() -> DocumentId {
    random_hash().into()
}

/// Generates random document view id.
#[fixture]
pub fn random_document_view_id() -> DocumentViewId {
    random_hash().into()
}
