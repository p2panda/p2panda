// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum PlainValue {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    StringOrRelation(String),
    PinnedRelationOrRelationList(Vec<String>),
    PinnedRelationList(Vec<Vec<String>>),
}

impl PlainValue {
    pub fn field_type(&self) -> &str {
        match self {
            PlainValue::Boolean(_) => "bool",
            PlainValue::Integer(_) => "int",
            PlainValue::Float(_) => "float",
            PlainValue::StringOrRelation(_) => "str",
            PlainValue::PinnedRelationOrRelationList(_) => "str[]",
            PlainValue::PinnedRelationList(_) => "str[][]",
        }
    }
}
