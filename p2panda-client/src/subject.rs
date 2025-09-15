// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::hash::Hash as StdHash;
use std::str::FromStr;

use p2panda_core::{Hash, HashError};
// @TODO: `TopicId` brings in a whole bunch of useless dependencies (iroh etc.) to compile.
use p2panda_net::TopicId;
use p2panda_sync::TopicQuery;
use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const SEPARATOR: char = '/';

#[derive(Clone, Debug, PartialEq, Eq, StdHash)]
pub struct Subject {
    root: Hash,
    suffixes: Vec<String>,
}

// @TODO: Add methods to match subjects.
impl Subject {
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

impl Display for Subject {
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

impl FromStr for Subject {
    type Err = SubjectError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut segments: VecDeque<&str> = value.split(SEPARATOR).collect();

        let Some(first_segment) = segments.pop_front() else {
            return Err(SubjectError::EmptyString);
        };

        Ok(Subject {
            root: Hash::from_str(first_segment)?,
            suffixes: segments.iter().map(|segment| segment.to_string()).collect(),
        })
    }
}

impl From<Hash> for Subject {
    fn from(hash: Hash) -> Self {
        Self {
            root: hash,
            suffixes: Vec::new(),
        }
    }
}

impl TryFrom<&str> for Subject {
    type Error = SubjectError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for Subject {
    type Error = SubjectError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl Serialize for Subject {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Subject {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SubjectVisitor;

        impl<'de> Visitor<'de> for SubjectVisitor {
            type Value = Subject;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("subject encoded as string")
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Subject::from_str(value).map_err(|err| serde::de::Error::custom(err))
            }
        }

        deserializer.deserialize_string(SubjectVisitor)
    }
}

// @TODO: This might change due to the p2panda-net refactor.
impl TopicQuery for Subject {}

// @TODO: This might change due to the p2panda-net refactor.
impl TopicId for Subject {
    fn id(&self) -> [u8; 32] {
        todo!()
    }
}

#[derive(Debug, Error)]
pub enum SubjectError {
    #[error("string is empty")]
    EmptyString,

    #[error("invalid root in subject: {0}")]
    InvalidRootId(#[from] HashError),
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use p2panda_core::Hash;

    use super::Subject;

    #[test]
    fn str_conversion() {
        let subject = Subject::from_hash(Hash::new(b"test")).with_suffix("/one/two/three");
        let subject_again = Subject::from_str(&subject.to_string()).unwrap();
        assert_eq!(subject, subject_again);
    }
}
