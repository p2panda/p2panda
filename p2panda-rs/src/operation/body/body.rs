// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation::body::plain::PlainFields;
use crate::operation::body::traits::Schematic;
use crate::operation::OperationFields;
use crate::schema::SchemaId;

#[derive(Clone, Debug, PartialEq)]
pub struct Body(pub Option<OperationFields>);

impl Body {
    pub fn plain_fields(&self) -> Option<PlainFields> {
        self.0.clone().map(|fields| (&fields).into())
    }
}
