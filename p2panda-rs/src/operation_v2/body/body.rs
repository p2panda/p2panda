// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation_v2::body::plain::PlainFields;
use crate::operation_v2::body::traits::Schematic;
use crate::operation_v2::OperationFields;
use crate::schema::SchemaId;

#[derive(Clone, Debug, PartialEq)]
pub struct Body(pub(crate) SchemaId, pub(crate) Option<OperationFields>);

impl Schematic for Body {
    fn schema_id(&self) -> &SchemaId {
        &self.0
    }

    fn plain_fields(&self) -> Option<PlainFields> {
        self.1.clone().map(|fields| (&fields).into())
    }
}
