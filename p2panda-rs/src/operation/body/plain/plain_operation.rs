// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use serde::de::Visitor;
use serde::{Deserialize, Serialize};

use crate::operation::body::plain::PlainFields;
use crate::operation::body::traits::Schematic;
use crate::operation::body::Body;
use crate::schema::SchemaId;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct PlainOperation(
    #[serde(skip_serializing_if = "Option::is_none")] pub(crate) Option<PlainFields>,
);

impl PlainOperation {
    pub fn plain_fields(&self) -> Option<PlainFields> {
        self.0.clone()
    }
}

impl From<&Body> for PlainOperation {
    fn from(body: &Body) -> Self {
        PlainOperation(
            body.plain_fields(),
        )
    }
}
