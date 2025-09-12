// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::hash::Hash as StdHash;
use std::str::FromStr;

use p2panda_core::{Hash, HashError};
// @TODO: `TopicId` brings in a whole bunch of useless dependencies (iroh etc.) to compile
use p2panda_net::TopicId;
use p2panda_sync::TopicQuery;
use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const SEPARATOR: char = '/';

#[derive(Clone, Debug, PartialEq, Eq, StdHash)]
pub struct Query {
    root: Hash,
    suffixes: Vec<String>,
}

impl Query {
    pub fn from_hash(root: Hash) -> Self {
        Self {
            root,
            suffixes: Vec::new(),
        }
    }

    // @TODO: Disallow special characters, spaces, etc.?
    pub fn with_suffix(mut self, value: &str) -> Self {
        let mut segments: Vec<String> = value.split(SEPARATOR).map(|val| val.to_string()).collect();
        self.suffixes.append(&mut segments);
        self
    }
}

impl Display for Query {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.root,
            SEPARATOR,
            self.suffixes.join(&SEPARATOR.to_string())
        )
    }
}

impl FromStr for Query {
    type Err = QueryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut segments: VecDeque<&str> = value.split(SEPARATOR).collect();

        let Some(first_segment) = segments.pop_front() else {
            return Err(QueryError::EmptyString);
        };

        Ok(Query {
            root: Hash::from_str(first_segment)?,
            suffixes: segments.iter().map(|segment| segment.to_string()).collect(),
        })
    }
}

impl From<Hash> for Query {
    fn from(hash: Hash) -> Self {
        Self {
            root: hash,
            suffixes: Vec::new(),
        }
    }
}

impl TryFrom<&str> for Query {
    type Error = QueryError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for Query {
    type Error = QueryError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl Serialize for Query {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Query {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct QueryVisitor;

        impl<'de> Visitor<'de> for QueryVisitor {
            type Value = Query;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("query encoded as string")
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Query::from_str(value).map_err(|err| serde::de::Error::custom(err))
            }
        }

        deserializer.deserialize_string(QueryVisitor)
    }
}

// @TODO: This might change due to the p2panda-net refactor.
impl TopicQuery for Query {}

// @TODO: This might change due to the p2panda-net refactor.
impl TopicId for Query {
    fn id(&self) -> [u8; 32] {
        todo!()
    }
}

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("string is empty")]
    EmptyString,

    #[error("invalid root in query: {0}")]
    InvalidRootId(#[from] HashError),
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use p2panda_core::Hash;

    use super::Query;

    #[test]
    fn str_conversion() {
        let query = Query::from_hash(Hash::new(b"test")).with_suffix("/one/two/three");
        let query_again = Query::from_str(&query.to_string()).unwrap();
        assert_eq!(query, query_again);
    }
}
