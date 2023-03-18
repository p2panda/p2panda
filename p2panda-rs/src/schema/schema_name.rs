// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Serialize, Serializer};

use crate::schema::error::SchemaNameError;
use crate::schema::validate::validate_name;
use crate::Validate;

/// A human readable schema name, used in the construction of `SchemaId`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SchemaName(String);

impl SchemaName {
    /// Construct and validate new schema name from a string.
    pub fn new(name: &str) -> Result<Self, SchemaNameError> {
        let name = Self(name.to_owned());
        name.validate()?;
        Ok(name)
    }
}

impl Validate for SchemaName {
    type Error = SchemaNameError;

    /// Validate that this schema name string follows the specification requirements.
    ///
    /// 1. It must be at most 64 characters long
    /// 2. It begins with a letter
    /// 3. It uses only alphanumeric characters, digits and the underscore character
    /// 4. It doesn't end with an underscore
    fn validate(&self) -> Result<(), Self::Error> {
        if !validate_name(&self.0) {
            return Err(SchemaNameError::MalformedSchemaName);
        }

        Ok(())
    }
}

impl FromStr for SchemaName {
    type Err = SchemaNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Display for SchemaName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for SchemaName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SchemaName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize into string
        let name: &str = Deserialize::deserialize(deserializer)?;

        // Check format
        let schema_name = SchemaName::new(name)
            .map_err(|err| serde::de::Error::custom(format!("invalid schema name, {}", err)))?;

        Ok(schema_name)
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use super::SchemaName;

    #[rstest]
    #[case("venues_with_garden")]
    #[case("animals_in_zoo_with_many_friends")]
    #[case("robot_3000_building_specification")]
    #[case("mushrooms_in_2054")]
    #[case("ILikeCamels")]
    #[case("AndDromedars")]
    #[case("And_Their_Special_Variants")]
    #[should_panic]
    #[case("where_did_we_end_up_again_")]
    #[should_panic]
    #[case("c0_1_2_1_a_b_4_____")]
    #[should_panic]
    #[case("")]
    #[should_panic]
    #[case("icecrüëmm")]
    #[should_panic]
    #[case("サービス！サービス！")]
    #[should_panic]
    #[case("schema_names_for_people_who_cant_decide_which_schema_name_to_pick")]
    #[should_panic]
    #[case("25_kangaroos")]
    #[should_panic]
    #[case("_and_how_did_it_all_began")]
    #[should_panic]
    #[case("???????")]
    #[should_panic]
    #[case("specification-says-no")]
    fn check_name_field(#[case] name_str: &str) {
        assert!(SchemaName::new(name_str).is_ok());
    }
}
