// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryFrom, TryInto};
use std::marker::PhantomData;
use std::str::FromStr;

use serde::de::Visitor;
use serde::Deserialize;

/// Visitor which can be used to deserialize a `String` or `u64` integer to a type T.
#[derive(Debug, Default)]
pub struct StringOrU64<T>(PhantomData<T>);

impl<T> StringOrU64<T> {
    pub fn new() -> Self {
        Self(PhantomData::<T>)
    }
}

impl<'de, T> Visitor<'de> for StringOrU64<T>
where
    T: Deserialize<'de> + FromStr + TryFrom<u64>,
{
    type Value = T;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("string or u64 integer")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let result = FromStr::from_str(value)
            .map_err(|_| serde::de::Error::custom("Invalid string value"))?;

        Ok(result)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let result = TryInto::<Self::Value>::try_into(value)
            .map_err(|_| serde::de::Error::custom("Invalid u64 value"))?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use serde::Deserialize;

    use super::StringOrU64;

    #[test]
    fn deserialize_str_and_u64() {
        #[derive(PartialEq, Eq, Debug)]
        struct Test(u64);

        impl From<u64> for Test {
            fn from(value: u64) -> Self {
                Self(value)
            }
        }

        impl FromStr for Test {
            type Err = Box<dyn std::error::Error>;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Test(u64::from_str(s)?))
            }
        }

        impl<'de> Deserialize<'de> for Test {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_any(StringOrU64::<Test>::new())
            }
        }

        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer("12", &mut cbor_bytes).unwrap();
        let result: Test = ciborium::de::from_reader(&cbor_bytes[..]).unwrap();
        assert_eq!(result, Test(12));
    }
}
