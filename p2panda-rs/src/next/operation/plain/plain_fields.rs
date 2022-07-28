// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::btree_map::Iter;
use std::collections::BTreeMap;
use std::fmt;

use serde::de::Visitor;
use serde::{Deserialize, Serialize};

use crate::next::operation::error::FieldsError;
use crate::next::operation::plain::PlainValue;
use crate::next::operation::{OperationFields, OperationValue};
use crate::next::schema::FieldName;

#[derive(Clone, Serialize, Default, Debug, PartialEq)]
pub struct PlainFields(BTreeMap<FieldName, PlainValue>);

impl PlainFields {
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

    pub fn get(&self, name: &str) -> Option<&PlainValue> {
        self.0.get(name)
    }

    pub fn insert(&mut self, name: &str, value: PlainValue) -> Result<(), FieldsError> {
        if self.0.contains_key(name) {
            Err(FieldsError::FieldDuplicate(name.to_owned()))
        } else {
            self.0.insert(name.to_owned(), value);
            Ok(())
        }
    }

    pub fn iter(&self) -> Iter<FieldName, PlainValue> {
        self.0.iter()
    }
}

impl<'de> Deserialize<'de> for PlainFields {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RawFieldsVisitor;

        impl<'de> Visitor<'de> for RawFieldsVisitor {
            type Value = PlainFields;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("p2panda operation fields")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut fields = PlainFields::new();
                let mut last_field_name: String = String::new();

                while let Some(field_name) = map.next_key::<String>()? {
                    // Check that field names are sorted lexicographically to ensure canonic
                    // encoding
                    if last_field_name.cmp(&field_name) == Ordering::Greater {
                        return Err(serde::de::Error::custom(format!(
                            "encountered unsorted field name: '{}' should be before '{}'",
                            field_name, last_field_name,
                        )));
                    }

                    let field_value: PlainValue = map.next_value()?;
                    fields.insert(&field_name, field_value).map_err(|_| {
                        // Fail if field names are duplicate to ensure canonic encoding
                        serde::de::Error::custom(format!(
                            "encountered duplicate field key '{}'",
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

impl From<&OperationFields> for PlainFields {
    fn from(fields: &OperationFields) -> Self {
        let mut raw = PlainFields::new();

        for (name, value) in fields.iter() {
            let raw_value = match value {
                OperationValue::Boolean(bool) => PlainValue::Boolean(*bool),
                OperationValue::Integer(int) => PlainValue::Integer(*int),
                OperationValue::Float(float) => PlainValue::Float(*float),
                OperationValue::String(str) => PlainValue::StringOrRelation(str.to_owned()),
                OperationValue::Relation(relation) => {
                    PlainValue::StringOrRelation(relation.document_id().as_str().to_owned())
                }
                OperationValue::RelationList(list) => PlainValue::PinnedRelationOrRelationList(
                    list.sorted()
                        .iter()
                        // @TODO: Improve conversion after `to_string` PR got merged
                        .map(|document_id| document_id.as_str().to_owned())
                        .collect(),
                ),
                OperationValue::PinnedRelation(relation) => {
                    PlainValue::PinnedRelationOrRelationList(
                        relation
                            .view_id()
                            .sorted()
                            .iter()
                            // @TODO: Improve conversion after `to_string` PR got merged
                            .map(|operation_id| operation_id.as_str().to_owned())
                            .collect(),
                    )
                }
                OperationValue::PinnedRelationList(list) => PlainValue::PinnedRelationList(
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
