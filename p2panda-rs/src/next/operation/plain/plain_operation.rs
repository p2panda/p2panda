// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use serde::de::Visitor;
use serde::{Deserialize, Serialize};

use crate::next::document::DocumentViewId;
use crate::next::operation::plain::PlainFields;
use crate::next::operation::traits::{Actionable, AsOperation, Schematic};
use crate::next::operation::{Operation, OperationAction, OperationVersion};
use crate::next::schema::SchemaId;

#[derive(Serialize, Debug, PartialEq)]
pub struct PlainOperation(
    OperationVersion,
    OperationAction,
    SchemaId,
    #[serde(skip_serializing_if = "Option::is_none")] Option<DocumentViewId>,
    #[serde(skip_serializing_if = "Option::is_none")] Option<PlainFields>,
);

impl Actionable for PlainOperation {
    fn version(&self) -> OperationVersion {
        self.0
    }

    fn action(&self) -> OperationAction {
        self.1
    }

    fn previous_operations(&self) -> Option<&DocumentViewId> {
        self.3.as_ref()
    }
}

impl Schematic for PlainOperation {
    fn schema_id(&self) -> &SchemaId {
        &self.2
    }

    fn fields(&self) -> Option<PlainFields> {
        self.4.clone()
    }
}

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

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let version: OperationVersion = seq.next_element()?.ok_or_else(|| {
                    serde::de::Error::custom("missing version field in operation format")
                })?;

                let action: OperationAction = seq.next_element()?.ok_or_else(|| {
                    serde::de::Error::custom("missing action field in operation format")
                })?;

                let schema_id: SchemaId = seq.next_element()?.ok_or_else(|| {
                    serde::de::Error::custom("missing schema id field in operation format")
                })?;

                let previous_operations = match action {
                    OperationAction::Create => None,
                    OperationAction::Update | OperationAction::Delete => {
                        let document_view_id: DocumentViewId =
                            seq.next_element()?.ok_or_else(|| {
                                serde::de::Error::custom(
                                    "missing previous_operations for this operation action",
                                )
                            })?;

                        Some(document_view_id)
                    }
                };

                let fields = match action {
                    OperationAction::Create | OperationAction::Update => {
                        let raw_fields: PlainFields = seq.next_element()?.ok_or_else(|| {
                            serde::de::Error::custom("missing fields for this operation action")
                        })?;

                        Some(raw_fields)
                    }
                    OperationAction::Delete => None,
                };

                match seq.size_hint() {
                    Some(items_left) => {
                        if items_left > 0 {
                            return Err(serde::de::Error::custom(
                                "too many items for this operation action",
                            ));
                        }
                    }
                    None => (),
                };

                Ok(PlainOperation(
                    version,
                    action,
                    schema_id,
                    previous_operations,
                    fields,
                ))
            }
        }

        deserializer.deserialize_seq(RawOperationVisitor)
    }
}

impl From<&Operation> for PlainOperation {
    fn from(operation: &Operation) -> Self {
        PlainOperation(
            AsOperation::version(operation),
            AsOperation::action(operation),
            AsOperation::schema_id(operation),
            AsOperation::previous_operations(operation),
            AsOperation::fields(operation)
                .as_ref()
                .map(|fields| fields.into()),
        )
    }
}
