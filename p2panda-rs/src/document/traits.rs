// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interfaces for interactions for document-like structs.

use crate::document::error::DocumentError;
use crate::document::{
    DocumentId, DocumentView, DocumentViewFields, DocumentViewId, DocumentViewValue,
};
use crate::identity_v2::PublicKey;
use crate::operation_v2::traits::AsOperation;
use crate::operation_v2::OperationValue;
use crate::schema::SchemaId;

/// Trait representing an "document-like" struct.
pub trait AsDocument {
    /// Get the document id.
    fn id(&self) -> &DocumentId;

    /// Get the document view id.
    fn view_id(&self) -> &DocumentViewId;

    /// Get the document author's public key.
    fn author(&self) -> &PublicKey;

    /// Get the document schema.
    fn schema_id(&self) -> &SchemaId;

    /// Get the fields of this document.
    fn fields(&self) -> Option<&DocumentViewFields>;

    /// Update the view of this document.
    fn update_view(&mut self, id: &DocumentViewId, view: Option<&DocumentViewFields>);

    /// Returns true if this document has applied an UPDATE operation.
    fn is_edited(&self) -> bool {
        match self.fields() {
            Some(fields) => fields.iter().any(|(_, document_view_value)| {
                &DocumentId::new(document_view_value.id()) != self.id()
            }),
            None => true,
        }
    }

    /// Returns true if this document has processed a DELETE operation.
    fn is_deleted(&self) -> bool {
        self.fields().is_none()
    }

    /// The current document view for this document. Returns None if this document
    /// has been deleted.
    fn view(&self) -> Option<DocumentView> {
        self.fields()
            .map(|fields| DocumentView::new(self.view_id(), fields))
    }

    /// Get the value for a field on this document.
    fn get(&self, key: &str) -> Option<&OperationValue> {
        if let Some(fields) = self.fields() {
            return fields.get(key).map(|view_value| view_value.value());
        }
        None
    }

    /// Update a documents current view with a single operation.
    ///
    /// For the update to be successful the passed operation must refer to this documents' current
    /// view id in it's previous field and must update a field which exists on this document.
    fn commit<T: AsOperation>(&mut self, operation: &T) -> Result<(), DocumentError> {
        // Validate operation passed to commit.
        if operation.is_create() {
            return Err(DocumentError::CommitCreate);
        }

        if operation.schema_id() != self.schema_id() {
            return Err(DocumentError::InvalidSchemaId(operation.id().to_owned()));
        }

        // Unwrap as all other operation types contain `previous`.
        let previous = operation.previous().unwrap();

        if self.is_deleted() {
            return Err(DocumentError::UpdateOnDeleted);
        }

        if self.view_id() != previous {
            return Err(DocumentError::PreviousDoesNotMatch(
                operation.id().to_owned(),
            ));
        }

        // We performed all validation commit the operation.
        self.commit_unchecked(operation);

        Ok(())
    }

    /// Commit an new operation to the document without performing any validation.
    fn commit_unchecked<T: AsOperation>(&mut self, operation: &T) {
        let next_fields = match operation.fields() {
            // If the operation contains fields it's an UPDATE and so we want to apply the changes
            // to the designated fields.
            Some(fields) => {
                // Get the current document fields, we can unwrap as we checked for deleted
                // documents above.
                let mut document_fields = self.fields().unwrap().to_owned();

                // For every field in the UPDATE operation update the relevant field in the
                // current document fields.
                for (name, value) in fields.iter() {
                    let document_field_value = DocumentViewValue::new(&operation.id(), value);

                    // We know all the fields are correct for this document as we checked the
                    // schema id above.
                    document_fields.insert(name, document_field_value);
                }

                // Return the updated fields.
                Some(document_fields)
            }
            // If the operation doesn't contain fields this must be a DELETE so we return None as we want to remove the
            // current document's fields.
            None => None,
        };

        // Construct the new document view id.
        let document_view_id = DocumentViewId::new(&[operation.id().to_owned()]);

        // Update the documents' view, edited/deleted state and view id.
        self.update_view(&document_view_id, next_fields.as_ref());
    }
}
