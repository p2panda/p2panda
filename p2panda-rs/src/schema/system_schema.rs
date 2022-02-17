// SPDX-License-Identifier: AGPL-3.0-or-later
use std::collections::BTreeMap;
use std::convert::TryFrom;

use crate::document::DocumentView;
use crate::operation::OperationValue;

use super::SystemSchemaError;

struct Schema(DocumentView);
struct SchemaField(DocumentView);

impl Schema {
    pub fn fields(&self) -> BTreeMap<String, OperationValue> {
        self.0.clone().into()
    }
}

impl SchemaField {
    pub fn fields(&self) -> BTreeMap<String, OperationValue> {
        self.0.clone().into()
    }
}

impl TryFrom<DocumentView> for Schema {
    type Error = SystemSchemaError;

    fn try_from(document_view: DocumentView) -> Result<Self, Self::Error> {
        // Validate correct field keys and types

        Ok(Self(document_view))
    }
}
