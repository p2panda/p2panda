// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::{DocumentId, DocumentView, DocumentViewFields, DocumentViewId};
use crate::identity::PublicKey;
use crate::operation::OperationValue;
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

    /// Returns true if this document has applied an UPDATE operation.
    fn is_edited(&self) -> bool;

    /// Returns true if this document has processed a DELETE operation.
    fn is_deleted(&self) -> bool;

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
}
