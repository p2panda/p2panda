// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::EntrySigned;
use crate::materialiser::{Edge, MaterialisationError};
use crate::operation::{AsOperation, OperationEncoded, OperationSigned};

/// Method for marshalling an array of entries and operations into an array of graph edges which
/// can then be turned into a DAG.
///
/// This serves as a helper to detach the DAG logic from any concepts of Entries and Operations.
/// The DAG just sees strings. This step will look different once have some more instance related
/// fields in our Operation struct.
pub fn marshall_entries(
    entries: Vec<(EntrySigned, OperationEncoded)>,
) -> Result<Vec<Edge>, MaterialisationError> {
    let mut edges = Vec::new();
    for (entry_signed, operation_encoded) in entries {
        let operation_signed = match OperationSigned::new(&entry_signed, &operation_encoded) {
            Ok(operation_signed) => Ok(operation_signed),
            Err(err) => Err(MaterialisationError::OperationSignedError(err)),
        }?;

        let (link, id) = match operation_signed.previous_operations() {
            Some(previous) => (
                // Just take the first previous operation as we don't
                // have a concept of multiple previous_operations
                // in this DAG implementation (it will be replaced by
                // `Document` in the near future).
                Some(previous[0].as_str().to_owned()),
                operation_signed.operation_id().as_str().to_owned(),
            ),
            None => (None, operation_signed.operation_id().as_str().to_owned()),
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
    use crate::test_utils::mocks::{send_to_node, Client, Node};

    use super::marshall_entries;

    #[rstest]
    fn marshall_entries_test(schema: Hash, key_pair: KeyPair) {
        let client = Client::new("panda".to_string(), key_pair);
        let mut node = Node::new();

        let (entry_1_hash, _) = send_to_node(
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
