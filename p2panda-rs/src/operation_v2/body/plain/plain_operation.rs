// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use serde::de::Visitor;
use serde::{Deserialize, Serialize};

use crate::operation_v2::body::plain::PlainFields;
use crate::operation_v2::body::Body;
use crate::operation_v2::body::traits::Schematic;
use crate::schema::SchemaId;

#[derive(Serialize, Debug, PartialEq)]
pub struct PlainOperation(
    SchemaId,
    #[serde(skip_serializing_if = "Option::is_none")] Option<PlainFields>,
);

impl<'de> Deserialize<'de> for PlainOperation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RawOperationVisitor;

        impl<'de> Visitor<'de> for RawOperationVisitor {
            type Value = PlainOperation;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("p2panda operation")
            }

            // @TODO: It would be nice to get more contextual error messages here, but sadly the
            // `ciborium::de::Error` types are hard to work with and we can not extract the
            // underlying error message easily (`Display` uses `Debug` internally).
            // See: https://docs.rs/ciborium/latest/src/ciborium/de/error.rs.html#62
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let schema_id: SchemaId = seq.next_element()?.ok_or_else(|| {
                    serde::de::Error::custom("missing schema id field in operation format")
                })?;

                let fields: Option<PlainFields> = seq.next_element()?.ok_or_else(|| {
                    serde::de::Error::custom("missing fields for this operation action")
                })?;

                if let Some(items_left) = seq.size_hint() {
                    if items_left > 0 {
                        return Err(serde::de::Error::custom(
                            "too many items for this operation action",
                        ));
                    }
                };

                Ok(PlainOperation(schema_id, fields))
            }
        }

        deserializer.deserialize_seq(RawOperationVisitor)
    }
}

impl From<&Body> for PlainOperation {
    fn from(body: &Body) -> Self {
        PlainOperation(
            Schematic::schema_id(body).to_owned(),
            Schematic::plain_fields(body).to_owned(),
        )
    }
}
