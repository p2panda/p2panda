// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::{decode_entry, EntrySigned};
use crate::materialiser::{Edge, MaterialisationError};
use crate::operation::OperationEncoded;

/// Method for marshalling an array of (EntrySigned, OperationEncoded) into an array of Edges which can then
/// be turned into a DAG. It is here to detach the DAG from any concepts of Entries and Operations. The DAG just sees
/// Strings. This step will look different once have some more instance related fields in our Operation struct.
pub fn marshall_entries(
    entries: Vec<(EntrySigned, OperationEncoded)>,
) -> Result<Vec<Edge>, MaterialisationError> {
    let mut edges = Vec::new();
    for (entry_signed, operation_encoded) in entries {
        let entry = match decode_entry(&entry_signed, Some(&operation_encoded)) {
            Ok(entry) => Ok(entry),
            Err(err) => Err(MaterialisationError::EntrySignedError(err)),
        }?;

        if entry.operation().is_none() {
            // The operation has been deleted.
            continue;
        }

        // `id` should not be optional (even CREATE operations should have it set) then
        // we wouldn't need the EntrySigned here at all.
        let (link, id) = match entry.operation().unwrap().id() {
            Some(_) => (
                Some(entry.backlink_hash().unwrap().as_str().to_owned()),
                entry_signed.hash().as_str().to_owned(),
            ),
            None => (None, entry_signed.hash().as_str().to_owned()),
        };
        edges.push((link, id));
    }
    Ok(edges)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::materialiser::Edge;
    use crate::operation::OperationValue;
    use crate::test_utils::fixtures::{
        create_operation, fields, key_pair, schema, update_operation,
    };
    use crate::test_utils::mocks::Client;
    use crate::test_utils::mocks::{send_to_node, Node};

    use super::marshall_entries;

    #[rstest]
    fn marshall_entries_test(schema: Hash, key_pair: KeyPair) {
        let client = Client::new("panda".to_string(), key_pair);
        let mut node = Node::new();

        let entry_1_hash = send_to_node(
            &mut node,
            &client,
            &create_operation(
                schema.clone(),
                fields(vec![(
                    "message",
                    OperationValue::Text("Hello!".to_string()),
                )]),
            ),
        )
        .unwrap();

        send_to_node(
            &mut node,
            &client,
            &update_operation(
                schema,
                entry_1_hash.clone(),
                vec![entry_1_hash],
                fields(vec![(
                    "operation",
                    OperationValue::Text("Hello too!".to_string()),
                )]),
            ),
        )
        .unwrap();

        let entries = node.all_entries();
        let entry_1 = entries.get(0).unwrap();
        let entry_2 = entries.get(1).unwrap();

        let edges = marshall_entries(vec![
            (entry_1.entry_encoded(), entry_1.operation_encoded()),
            (entry_2.entry_encoded(), entry_2.operation_encoded()),
        ])
        .unwrap();

        let edge_1: Edge = (None, entry_1.hash_str());
        let edge_2: Edge = (Some(entry_1.hash_str()), entry_2.hash_str());

        assert_eq!(edges, vec![edge_1, edge_2]);
    }
}
