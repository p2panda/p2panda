// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::btree_map::Iter;
use std::collections::BTreeMap;
use std::fmt;

use serde::de::Visitor;
use serde::{Deserialize, Serialize};

use crate::document::DocumentViewId;
use crate::operation::{
    AsOperation, Operation, OperationAction, OperationFields, OperationValue, OperationVersion,
    RawOperationError,
};
use crate::schema::{FieldName, SchemaId};

#[derive(Serialize, Default, Debug, PartialEq)]
pub struct RawFields(BTreeMap<FieldName, RawValue>);

impl RawFields {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns true when no field is given.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of fields.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, name: &str) -> Option<&RawValue> {
        self.0.get(name)
    }

    pub fn insert(&mut self, name: &str, value: RawValue) -> Result<(), RawOperationError> {
        if self.0.contains_key(name) {
            // @TODO
            panic!("Duplicate")
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    pub fn iter(&self) -> Iter<FieldName, RawValue> {
        self.0.iter()
    }
}

impl<'de> Deserialize<'de> for RawFields {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RawFieldsVisitor;

        impl<'de> Visitor<'de> for RawFieldsVisitor {
            type Value = RawFields;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("p2panda operation fields")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut fields = RawFields::new();
                let mut last_field_name: String = String::new();

                while let Some(field_name) = map.next_key::<String>()? {
                    // Check that field names are sorted lexicographically to ensure canonic
                    // encoding
                    if last_field_name.cmp(&field_name) == Ordering::Greater {
                        return Err(serde::de::Error::custom(format!(
                            "Encountered unsorted field name: '{}' should be before '{}'",
                            field_name, last_field_name,
                        )));
                    }

                    let field_value: RawValue = map.next_value()?;
                    fields.insert(&field_name, field_value).map_err(|_| {
                        // Fail if field names are duplicate to ensure canonic encoding
                        serde::de::Error::custom(format!(
                            "Encountered duplicate field key '{}'",
                            field_name
                        ))
                    })?;

                    last_field_name = field_name;
                }

                Ok(fields)
            }
        }

        deserializer.deserialize_map(RawFieldsVisitor)
    }
}

impl From<&OperationFields> for RawFields {
    fn from(fields: &OperationFields) -> Self {
        let mut raw = RawFields::new();

        for (name, value) in fields.iter() {
            let raw_value = match value {
                OperationValue::Boolean(bool) => RawValue::Boolean(*bool),
                OperationValue::Integer(int) => RawValue::Integer(*int),
                OperationValue::Float(float) => RawValue::Float(*float),
                OperationValue::Text(str) => RawValue::TextOrRelation(str.to_owned()),
                OperationValue::Relation(relation) => {
                    RawValue::TextOrRelation(relation.document_id().as_str().to_owned())
                }
                OperationValue::RelationList(list) => RawValue::PinnedRelationOrRelationList(
                    list.sorted()
                        .iter()
                        // @TODO: Improve conversion after `to_string` PR got merged
                        .map(|document_id| document_id.as_str().to_owned())
                        .collect(),
                ),
                OperationValue::PinnedRelation(relation) => RawValue::PinnedRelationOrRelationList(
                    relation
                        .view_id()
                        .sorted()
                        .iter()
                        // @TODO: Improve conversion after `to_string` PR got merged
                        .map(|operation_id| operation_id.as_str().to_owned())
                        .collect(),
                ),
                OperationValue::PinnedRelationList(list) => RawValue::PinnedRelationList(
                    list.sorted()
                        .iter()
                        .map(|document_view_id| {
                            document_view_id
                                .sorted()
                                .iter()
                                // @TODO: Improve conversion after `to_string` PR got merged
                                .map(|operation_id| operation_id.as_str().to_owned())
                                .collect()
                        })
                        .collect(),
                ),
            };

            // Unwrap here because we already know that there are no duplicates in
            // `OperationFields`
            raw.insert(name, raw_value).unwrap();
        }

        raw
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum RawValue {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    TextOrRelation(String),
    PinnedRelationOrRelationList(Vec<String>),
    PinnedRelationList(Vec<Vec<String>>),
}

impl RawValue {
    pub fn field_type(&self) -> &str {
        match self {
            RawValue::Boolean(_) => "bool",
            RawValue::Integer(_) => "int",
            RawValue::Float(_) => "float",
            RawValue::TextOrRelation(_) => "str",
            RawValue::PinnedRelationOrRelationList(_) => "str[]",
            RawValue::PinnedRelationList(_) => "str[][]",
        }
    }
}

#[derive(Serialize, Debug, PartialEq)]
pub struct RawOperation(
    OperationVersion,
    OperationAction,
    SchemaId,
    #[serde(skip_serializing_if = "Option::is_none")] Option<DocumentViewId>,
    #[serde(skip_serializing_if = "Option::is_none")] Option<RawFields>,
);

impl RawOperation {
    pub fn version(&self) -> OperationVersion {
        self.0
    }

    pub fn action(&self) -> OperationAction {
        self.1
    }

    pub fn schema_id(&self) -> &SchemaId {
        &self.2
    }

    pub fn previous_operations(&self) -> Option<&DocumentViewId> {
        self.3.as_ref()
    }

    pub fn fields(&self) -> Option<&RawFields> {
        self.4.as_ref()
    }
}

impl<'de> Deserialize<'de> for RawOperation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RawOperationVisitor;

        impl<'de> Visitor<'de> for RawOperationVisitor {
            type Value = RawOperation;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("p2panda operation")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let version: OperationVersion = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Missing version field"))?;

                let action: OperationAction = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Missing action field"))?;

                let schema_id: SchemaId = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Missing schema field"))?;

                let previous_operations = match action {
                    OperationAction::Create => None,
                    OperationAction::Update | OperationAction::Delete => {
                        let document_view_id: DocumentViewId =
                            seq.next_element()?.ok_or_else(|| {
                                serde::de::Error::custom("Missing previous_operations field")
                            })?;

                        Some(document_view_id)
                    }
                };

                let fields = match action {
                    OperationAction::Create | OperationAction::Update => {
                        let raw_fields: RawFields = seq
                            .next_element()?
                            .ok_or_else(|| serde::de::Error::custom("Missing fields"))?;

                        Some(raw_fields)
                    }
                    OperationAction::Delete => None,
                };

                Ok(RawOperation(
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

impl From<&Operation> for RawOperation {
    fn from(operation: &Operation) -> Self {
        RawOperation(
            operation.version(),
            operation.action(),
            operation.schema(),
            operation.previous_operations(),
            operation.fields().as_ref().map(|fields| fields.into()),
        )
    }
}
