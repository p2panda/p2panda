// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods to reduce a list of operations into a single view.
use crate::document::error::DocumentBuilderError;
use crate::document::{DocumentViewFields, DocumentViewValue, IsDeleted, IsEdited};
use crate::graph::Graph;
use crate::operation::traits::{AsOperation, AsVerifiedOperation};
use crate::operation::{OperationId, VerifiedOperation};

/// Construct a graph from a list of operations.
pub(crate) fn build_graph(
    operations: &[VerifiedOperation],
) -> Result<Graph<OperationId, VerifiedOperation>, DocumentBuilderError> {
    let mut graph = Graph::new();

    // Add all operations to the graph.
    for operation in operations {
        graph.add_node(operation.operation_id(), operation.clone());
    }

    // Add links between operations in the graph.
    for operation in operations {
        if let Some(previous_operations) = operation.previous_operations() {
            for previous in previous_operations.iter() {
                let success = graph.add_link(previous, operation.operation_id());
                if !success {
                    return Err(DocumentBuilderError::InvalidOperationLink(
                        operation.operation_id().to_owned(),
                    ));
                }
            }
        }
    }

    Ok(graph)
}

/// Reduce a list of operations into a single view.
///
/// Returns the reduced fields of a document view along with the `edited` and `deleted` boolean
/// flags. If the document contains a DELETE operation, then no view is returned and the `deleted`
/// flag is set to true. If the document contains one or more UPDATE operations, then the reduced
/// view is returned and the `edited` flag is set to true.
pub(crate) fn reduce(
    ordered_operations: &[VerifiedOperation],
) -> (Option<DocumentViewFields>, IsEdited, IsDeleted) {
    let mut is_edited = false;

    let mut document_view_fields = DocumentViewFields::new();

    for operation in ordered_operations {
        if operation.is_delete() {
            return (None, true, true);
        }

        if operation.is_update() {
            is_edited = true
        }

        if let Some(fields) = operation.fields() {
            for (key, value) in fields.iter() {
                let document_view_value = DocumentViewValue::new(operation.operation_id(), value);
                document_view_fields.insert(key, document_view_value);
            }
        }
    }

    (Some(document_view_fields), is_edited, false)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::operation::{OperationValue, VerifiedOperation};
    use crate::test_utils::fixtures::{
        operation_fields, random_previous_operations, verified_operation_with_schema,
    };

    use super::reduce;

    #[rstest]
    fn reduces_operations(
        #[from(verified_operation_with_schema)] create_operation: VerifiedOperation,
        #[from(verified_operation_with_schema)]
        #[with(
            Some(operation_fields(vec![("username", OperationValue::String("Yahooo!".into()))])),
            Some(random_previous_operations(1))
        )]
        update_operation: VerifiedOperation,
        #[from(verified_operation_with_schema)]
        #[with(None, Some(random_previous_operations(1)))]
        delete_operation: VerifiedOperation,
    ) {
        let (reduced_create, is_edited, is_deleted) = reduce(&[create_operation.clone()]);
        assert_eq!(
            *reduced_create.unwrap().get("username").unwrap().value(),
            OperationValue::String("bubu".to_string())
        );
        assert!(!is_edited);
        assert!(!is_deleted);

        let (reduced_update, is_edited, is_deleted) =
            reduce(&[create_operation.clone(), update_operation.clone()]);
        assert_eq!(
            *reduced_update.unwrap().get("username").unwrap().value(),
            OperationValue::String("Yahooo!".to_string())
        );
        assert!(is_edited);
        assert!(!is_deleted);

        let (reduced_delete, is_edited, is_deleted) =
            reduce(&[create_operation, update_operation, delete_operation]);

        // The value remains the same, but the deleted flag is true now.
        assert!(reduced_delete.is_none());
        assert!(is_edited);
        assert!(is_deleted);
    }
}
