// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods to reduce a list of operations into a single view.
use crate::document::error::DocumentBuilderError;
use crate::document::{DocumentViewFields, DocumentViewValue};
use crate::graph::Graph;
use crate::identity_v2::PublicKey;
use crate::operation_v2::traits::AsOperation;
use crate::operation_v2::{Operation, OperationId};

/// Construct a graph from a list of operations.
pub fn build_graph(
    operations: &[(OperationId, Operation, PublicKey)],
) -> Result<Graph<OperationId, (OperationId, Operation, PublicKey)>, DocumentBuilderError> {
    let mut graph = Graph::new();

    // Add all operations to the graph.
    for (id, operation, public_key) in operations {
        graph.add_node(id, (id.to_owned(), operation.to_owned(), *public_key));
    }

    // Add links between operations in the graph.
    for (id, operation, _public_key) in operations {
        if let Some(previous) = operation.previous() {
            for previous in previous.iter() {
                let success = graph.add_link(previous, id);
                if !success {
                    return Err(DocumentBuilderError::InvalidOperationLink(id.to_owned()));
                }
            }
        }
    }

    Ok(graph)
}

/// Reduce a list of operations into a single view.
///
/// Returns the reduced fields of a document view wrapped in an Option. If the passed operations
/// contain a DELETE then the returned fields will be None.
pub fn reduce(
    ordered_operations: &[(OperationId, Operation, PublicKey)],
) -> Option<DocumentViewFields> {
    let mut document_view_fields = DocumentViewFields::new();

    for (id, operation, _public_key) in ordered_operations {
        if operation.is_delete() {
            return None;
        }

        if let Some(fields) = operation.fields() {
            for (key, value) in fields.iter() {
                let document_view_value = DocumentViewValue::new(id, value);
                document_view_fields.insert(key, document_view_value);
            }
        }
    }

    Some(document_view_fields)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::identity_v2::PublicKey;
    use crate::operation_v2::{Operation, OperationValue};
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, public_key, random_operation_id, update_operation,
    };

    use super::reduce;

    #[rstest]
    fn reduces_operations(
        #[with(vec![("username", OperationValue::String("bubu".into()))])]
        create_operation: Operation,
        #[with(vec![("username", OperationValue::String("Yahooo!".into()))])]
        update_operation: Operation,
        delete_operation: Operation,
        public_key: PublicKey,
    ) {
        let mut operations = Vec::new();
        operations.push((random_operation_id(), create_operation, public_key));
        let reduced_create = reduce(&operations);

        assert_eq!(
            *reduced_create.unwrap().get("username").unwrap().value(),
            OperationValue::String("bubu".to_string())
        );

        operations.push((random_operation_id(), update_operation, public_key));
        let reduced_update = reduce(&operations);

        assert_eq!(
            *reduced_update.unwrap().get("username").unwrap().value(),
            OperationValue::String("Yahooo!".to_string())
        );

        operations.push((random_operation_id(), delete_operation, public_key));
        let reduced_delete = reduce(&operations);

        assert!(reduced_delete.is_none());
    }
}
