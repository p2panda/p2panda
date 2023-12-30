// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation::body::plain::PlainFields;
use crate::operation::body::traits::Schematic;
use crate::operation::OperationFields;
use crate::schema::SchemaId;

#[derive(Clone, Debug, PartialEq)]
pub struct Body(pub SchemaId, pub Option<OperationFields>);

impl Schematic for Body {
    fn schema_id(&self) -> &SchemaId {
        &self.0
    }

    fn plain_fields(&self) -> Option<PlainFields> {
        self.1.clone().map(|fields| (&fields).into())
    }
}
