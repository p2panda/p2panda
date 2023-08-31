// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use serde::de::Visitor;
use serde::{Deserialize, Serialize};

use crate::document::DocumentViewId;
use crate::operation::plain::PlainFields;
use crate::operation::traits::{Actionable, AsOperation, Schematic};
use crate::operation::{Operation, OperationAction, OperationVersion};
use crate::schema::SchemaId;

/// Intermediate operation type which has been decoded, but not checked against a schema.
///
/// Use plain operations to already read important data from them, like the schema id or operation
/// action.
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

    fn previous(&self) -> Option<&DocumentViewId> {
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

            // @TODO: It would be nice to get more contextual error messages here, but sadly the
            // `ciborium::de::Error` types are hard to work with and we can not extract the
            // underlying error message easily (`Display` uses `Debug` internally).
            // See: https://docs.rs/ciborium/latest/src/ciborium/de/error.rs.html#62
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

                let previous = match action {
                    OperationAction::Create => None,
                    OperationAction::Update | OperationAction::Delete => {
                        let document_view_id: DocumentViewId =
                            seq.next_element()?.ok_or_else(|| {
                                serde::de::Error::custom(
                                    "missing previous for this operation action",
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

                if let Some(items_left) = seq.size_hint() {
                    if items_left > 0 {
                        return Err(serde::de::Error::custom(
                            "too many items for this operation action",
                        ));
                    }
                };

                Ok(PlainOperation(version, action, schema_id, previous, fields))
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
            AsOperation::previous(operation),
            AsOperation::fields(operation)
                .as_ref()
                .map(|fields| fields.into()),
        )
    }
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ciborium::value::{Error, Value};
    use rstest::rstest;
    use serde_bytes::ByteBuf;

    use crate::document::DocumentViewId;
    use crate::operation::traits::{Actionable, Schematic};
    use crate::operation::{Operation, OperationAction, OperationId, OperationVersion};
    use crate::schema::{SchemaId, SchemaName};
    use crate::serde::{deserialize_into, serialize_from, serialize_value};
    use crate::test_utils::fixtures::{
        document_view_id, operation_with_schema, random_operation_id, schema_name,
    };

    use super::PlainOperation;

    #[rstest]
    fn from_operation(#[from(operation_with_schema)] operation: Operation) {
        let plain_operation = PlainOperation::from(&operation);
        assert_eq!(plain_operation.action(), operation.action());
        assert_eq!(plain_operation.version(), operation.version());
        assert_eq!(plain_operation.schema_id(), operation.schema_id());
        assert_eq!(plain_operation.fields(), operation.fields());
        assert_eq!(plain_operation.previous(), operation.previous());
    }

    #[rstest]
    fn serialize(document_view_id: DocumentViewId, #[with("mushrooms")] schema_name: SchemaName) {
        assert_eq!(
            serialize_from(PlainOperation(
                OperationVersion::V1,
                OperationAction::Create,
                SchemaId::Application(schema_name.clone(), document_view_id.clone()),
                None,
                Some(vec![("name", "Hericium coralloides".into())].into())
            )),
            serialize_value(cbor!(
                [1, 0, format!("{schema_name}_{document_view_id}"), {
                    "name" => ByteBuf::from("Hericium coralloides".as_bytes())
                }]
            ))
        );
    }

    #[rstest]
    fn deserialize(
        document_view_id: DocumentViewId,
        random_operation_id: OperationId,
        #[with("mushrooms")] schema_name: SchemaName,
    ) {
        assert_eq!(
            deserialize_into::<PlainOperation>(&serialize_value(cbor!(
                [1, 1, format!("{schema_name}_{document_view_id}"), [random_operation_id.to_string()], {
                    "name" => ByteBuf::from("Lycoperdon echinatum".as_bytes())
                }]
            )))
            .unwrap(),
            PlainOperation(
                OperationVersion::V1,
                OperationAction::Update,
                SchemaId::Application(schema_name, document_view_id),
                Some(DocumentViewId::from(random_operation_id)),
                Some(vec![("name", "Lycoperdon echinatum".into())].into())
            )
        );
    }

    #[rstest]
    #[should_panic(expected = "missing version field in operation format")]
    #[case::no_fields(cbor!([]))]
    #[should_panic(expected = "missing action field in operation format")]
    #[case::only_version(cbor!([1]))]
    #[should_panic(expected = "missing schema id field in operation format")]
    #[case::only_version_and_action(cbor!([1, 1]))]
    #[should_panic(expected = "invalid type: string, expected integer")]
    #[case::incorrect_type(cbor!(["Test"]))]
    #[should_panic(expected = "missing fields for this operation action")]
    #[case::missing_fields(cbor!([1, 0, "schema_field_definition_v1"]))]
    #[should_panic(expected = "invalid hash length 2 bytes, expected 34 bytes")]
    #[case::hash_too_small(cbor!([1, 1, "schema_field_definition_v1", ["0020"]]))]
    #[should_panic(expected = "invalid type: map, expected array")]
    #[case::fields_wrong_type(cbor!([1, 1, "schema_field_definition_v1", { "type" => "int" }]))]
    fn deserialize_invalid_operations(#[case] cbor: Result<Value, Error>) {
        // Check the cbor is valid.
        assert!(cbor.is_ok());

        // Deserialize into a plain operation, we unwrap here to cause a panic and then test for
        // expected error stings.
        deserialize_into::<PlainOperation>(&serialize_value(cbor)).unwrap();
    }

    #[rstest]
    #[should_panic(expected = " name contains too many or invalid characters")]
    #[case::really_wrong_schema_name("Really Wrong Schema Name?!")]
    #[should_panic(expected = "name contains too many or invalid characters")]
    #[case::schema_name_ends_with_underscore("schema_name_ends_with_underscore_")]
    #[should_panic(expected = "name contains too many or invalid characters")]
    #[case::schema_name_invalid_char("$_$_$")]
    #[should_panic(expected = "name contains too many or invalid characters")]
    #[case::schema_name_too_long(
        "really_really_really_really_really_really_really_really_long_name"
    )]
    #[should_panic(expected = "name contains too many or invalid characters")]
    #[case::panda_face_emojis_not_allowed(
        "🐼" // We can only dream ;-p
    )]
    fn deserialize_operation_with_invalid_name_in_schema_id(#[case] schema_name: &str) {
        // Encode operation as cbor using the passed name combined with a valid hash to make a
        // schema id.
        let operation_cbor = cbor!([1, 1, format!("{schema_name}_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"), [{ "type" => "int" }]]);

        // Check the cbor is valid.
        assert!(operation_cbor.is_ok());

        // Deserialize into a plain operation, we unwrap here to cause a panic and then test for
        // expected error stings.
        deserialize_into::<PlainOperation>(&serialize_value(operation_cbor)).unwrap();
    }
}
