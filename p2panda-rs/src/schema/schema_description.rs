// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Serialize, Serializer};

use crate::schema::error::SchemaDescriptionError;
use crate::schema::validate::validate_description;
use crate::Validate;

/// The description of a schema which adheres to specification requirements. Used in the
/// construction of `Schema`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SchemaDescription(String);

impl SchemaDescription {
    /// Construct and validate new schema description from a string.
    pub fn new(description: &str) -> Result<Self, SchemaDescriptionError> {
        let description = Self(description.to_owned());
        description.validate()?;
        Ok(description)
    }
}

impl Validate for SchemaDescription {
    type Error = SchemaDescriptionError;

    /// Perform validation on the description string.
    ///
    /// 1. It consists of unicode characters
    /// 2. ... and must be at most 256 characters long
    fn validate(&self) -> Result<(), Self::Error> {
        if !validate_description(&self.0) {
            return Err(SchemaDescriptionError::TooLongSchemaDescription);
        }
        Ok(())
    }
}

impl FromStr for SchemaDescription {
    type Err = SchemaDescriptionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Display for SchemaDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for SchemaDescription {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SchemaDescription {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize into string
        let name: &str = Deserialize::deserialize(deserializer)?;

        // Check format
        let schema_name = SchemaDescription::new(name).map_err(|err| {
            serde::de::Error::custom(format!("invalid schema description, {}", err))
        })?;

        Ok(schema_name)
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use super::SchemaDescription;

    #[rstest]
    #[case(
        "The kangaroo is a marsupial from the family Macropodidae
           (macropods, meaning large foot)"
    )]
    #[case("%*&______@@@@@[[}}}{}}}}}}}&}{&{&{&{&{&}}}}}]]")]
    #[should_panic]
    #[case(
        "In common use the term is used to describe the largest species from this
           family, the red kangaroo, as well as the antilopine kangaroo, eastern grey
           kangaroo, and western grey kangaroo! Kangaroos have large, powerful hind legs,
           large feet adapted for leaping, a long muscular tail for balance, and a small
           head. Like most marsupials, female kangaroos have a pouch called a marsupium
           in which joeys complete postnatal development."
    )]
    fn validates_descriptions(#[case] description_str: &str) {
        assert!(SchemaDescription::new(description_str).is_ok());
    }
}
