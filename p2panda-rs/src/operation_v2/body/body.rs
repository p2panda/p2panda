// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use serde::de::Visitor;
use serde::{Deserialize, Serialize};

use crate::operation_v2::body::PlainFields;
use crate::operation_v2::traits::{AsOperation, Schematic};
use crate::operation_v2::Operation;
use crate::schema::SchemaId;

#[derive(Serialize, Debug, PartialEq)]
pub struct Body(
    pub(crate) SchemaId,
    #[serde(skip_serializing_if = "Option::is_none")] pub(crate) Option<PlainFields>,
);

impl Schematic for Body {
    fn schema_id(&self) -> &SchemaId {
        &self.0
    }

    fn fields(&self) -> Option<PlainFields> {
        self.1.clone()
    }
}

impl<'de> Deserialize<'de> for Body {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RawOperationVisitor;

        impl<'de> Visitor<'de> for RawOperationVisitor {
            type Value = Body;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("p2panda operation")
            }

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

                Ok(Body(schema_id, fields))
            }
        }

        deserializer.deserialize_seq(RawOperationVisitor)
    }
}

impl From<&Operation> for Body {
    fn from(operation: &Operation) -> Self {
        Body(
            AsOperation::schema_id(operation),
            AsOperation::fields(operation)
                .as_ref()
                .map(|fields| fields.into()),
        )
    }
}
