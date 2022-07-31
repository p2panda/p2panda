// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryFrom, TryInto};
use std::marker::PhantomData;
use std::str::FromStr;

use arrayvec::ArrayVec;
use bamboo_rs_core_ed25519_yasmf::Entry as BambooEntry;
use serde::de::Visitor;
use serde::Deserialize;

use crate::entry::{Entry, EntrySigned, EntrySignedError, LogId, SeqNum, SIGNATURE_SIZE};
use crate::hash::{Hash, HASH_SIZE};
use crate::operation::{Operation, OperationEncoded};

/// Method to decode an entry and optionally its payload.
///
/// Takes [`EntrySigned`] and optionally [`OperationEncoded`] as arguments, returns a decoded and
/// unsigned [`Entry`].
///
/// Entries are separated from the operations they refer to and serve as "off-chain data". Since
/// operations can independently be deleted they have to be passed on as an optional argument.
///
/// When a [`OperationEncoded`] is passed it will automatically check its integrity with this
/// [`Entry`] by comparing their hashes. Valid operations will be included in the returned
/// [`Entry`], if an invalid operation is passed an error will be returned.
pub fn decode_entry(
    entry_encoded: &EntrySigned,
    operation_encoded: Option<&OperationEncoded>,
) -> Result<Entry, EntrySignedError> {
    let entry: BambooEntry<ArrayVec<[u8; HASH_SIZE]>, ArrayVec<[u8; SIGNATURE_SIZE]>> =
        entry_encoded.into();

    let operation = match operation_encoded {
        Some(payload) => {
            entry_encoded.validate_operation(payload)?;
            Some(Operation::from(payload))
        }
        None => None,
    };

    let entry_hash_backlink: Option<Hash> = entry.backlink.map(|link| (&link).into());
    let entry_hash_skiplink: Option<Hash> = entry.lipmaa_link.map(|link| (&link).into());

    Ok(Entry::new(
        &LogId::new(entry.log_id),
        operation.as_ref(),
        entry_hash_skiplink.as_ref(),
        entry_hash_backlink.as_ref(),
        &SeqNum::new(entry.seq_num).unwrap(),
    )
    .unwrap())
}

/// Visitor which can be used to deserialize a `String` or `u64` integer to a type T.
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

