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

// @TODO: Needs refactoring
/* #[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::convert::TryInto;

    use proptest::prelude::prop;
    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::entry::{sign_and_encode, Entry, EntrySigned};
    use crate::identity::KeyPair;
    use crate::operation::OperationEncoded;
    use crate::test_utils::constants::{test_fields, HASH, PRIVATE_KEY};
    use crate::test_utils::fixtures::{
        entry_signed_encoded, entry_signed_encoded_unvalidated, key_pair, operation,
        operation_encoded, operation_fields, random_hash,
    };
    use crate::test_utils::templates::many_valid_entries;

    #[rstest]
    fn string_representation(entry_signed_encoded: EntrySigned) {
        assert_eq!(
            entry_signed_encoded.as_str(),
            &entry_signed_encoded.to_string()
        );
    }

    #[rstest]
    fn test_entry_signed(entry_signed_encoded: EntrySigned, key_pair: KeyPair) {
        let verification = KeyPair::verify(
            key_pair.public_key(),
            &entry_signed_encoded.unsigned_bytes(),
            &entry_signed_encoded.signature(),
        );
        assert!(verification.is_ok(), "{:?}", verification.unwrap_err())
    }

    #[rstest]
    fn test_size(entry_signed_encoded: EntrySigned) {
        let size: usize = entry_signed_encoded.size().try_into().unwrap();
        assert_eq!(size, entry_signed_encoded.to_bytes().len())
    }

    #[rstest]
    fn test_payload_hash(entry_signed_encoded: EntrySigned, operation_encoded: OperationEncoded) {
        let expected_payload_hash = operation_encoded.hash();
        assert_eq!(entry_signed_encoded.payload_hash(), expected_payload_hash)
    }

    #[rstest]
    #[case::valid_first_entry(entry_signed_encoded_unvalidated(
        1,
        1,
        None,
        None,
        Some(operation(Some(operation_fields(test_fields())), None, None)),
        key_pair(PRIVATE_KEY)
    ))]
    #[case::valid_entry_with_backlink(entry_signed_encoded_unvalidated(
        2,
        1,
        Some(random_hash()),
        None,
        Some(operation(Some(operation_fields(test_fields())), None, None)),
        key_pair(PRIVATE_KEY)
    ))]
    #[case::valid_entry_with_skiplink_and_backlink(entry_signed_encoded_unvalidated(
        13,
        1,
        Some(random_hash()),
        Some(random_hash()),
        Some(operation(Some(operation_fields(test_fields())), None, None)),
        key_pair(PRIVATE_KEY)
    ))]
    #[case::skiplink_ommitted_when_sam_as_backlink(entry_signed_encoded_unvalidated(
        14,
        1,
        Some(random_hash()),
        None,
        Some(operation(Some(operation_fields(test_fields())), None, None)),
        key_pair(PRIVATE_KEY)
    ))]
    fn validate(#[case] entry_signed_encoded_unvalidated: String) {
        assert!(EntrySigned::new(&entry_signed_encoded_unvalidated).is_ok());
    }

    #[rstest]
    #[case::empty_string("", "Bytes to decode had length of 0")]
    #[case::invalid_hex_string("123456789Z", "invalid hex encoding in entry")]
    #[case::another_invalid_hex_string(":{][[5£$%*(&*££  ++`/.", "invalid hex encoding in entry")]
    #[case::seq_number_zero(
        entry_signed_encoded_unvalidated(
            0,
            1,
            None,
            None,
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Entry sequence must be larger than 0 but was 0"
    )]
    #[case::should_not_have_skiplink(
        entry_signed_encoded_unvalidated(
            1,
            1,
            None,
            Some(random_hash()),
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode payload hash DecodeError"
    )]
    #[case::should_not_have_backlink(
        entry_signed_encoded_unvalidated(
            1,
            1,
            Some(random_hash()),
            None,
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode payload hash DecodeError"
    )]
    #[case::should_not_have_backlink_or_skiplink(
        entry_signed_encoded_unvalidated(
                1,
                1,
                Some(HASH.parse().unwrap()),
                Some(HASH.parse().unwrap()),
                Some(operation(Some(operation_fields(test_fields())), None, None))
,
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode payload hash DecodeError"
    )]
    #[case::missing_backlink(
        entry_signed_encoded_unvalidated(
            2,
            1,
            None,
            None,
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode backlink yamf hash: DecodeError"
    )]
    #[case::missing_skiplink(
        entry_signed_encoded_unvalidated(
            8,
            1,
            Some(random_hash()),
            None,
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode backlink yamf hash: DecodeError"
    )]
    #[case::should_not_include_skiplink(
        entry_signed_encoded_unvalidated(
            14,
            1,
            Some(HASH.parse().unwrap()),
            Some(HASH.parse().unwrap()),
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode payload hash DecodeError"
    )]
    #[case::payload_hash_and_size_missing(
        entry_signed_encoded_unvalidated(
            14,
            1,
            Some(random_hash()),
            Some(HASH.parse().unwrap()),
            None,
            key_pair(PRIVATE_KEY)
        ),
        "Could not decode payload hash DecodeError"
    )]
    #[case::skiplink_and_backlink_should_be_unique(
        entry_signed_encoded_unvalidated(
            13,
            1,
            Some(HASH.parse().unwrap()),
            Some(HASH.parse().unwrap()),
            Some(operation(Some(operation_fields(test_fields())), None, None)),
            key_pair(PRIVATE_KEY)
        ),
        "backlink and skiplink are identical"
    )]
    fn correct_errors_on_invalid_entries(
        #[case] entry_signed_encoded_unvalidated: String,
        #[case] expected_error_message: &str,
    ) {
        assert_eq!(
            EntrySigned::new(&entry_signed_encoded_unvalidated)
                .unwrap_err()
                .to_string(),
            expected_error_message
        );
    }

    #[apply(many_valid_entries)]
    fn it_hashes(#[case] entry: Entry, key_pair: KeyPair) {
        let entry_first_encoded = sign_and_encode(&entry, &key_pair).unwrap();
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&entry_first_encoded, key_value.clone());
        let key_value_retrieved = hash_map.get(&entry_first_encoded).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }

    proptest! {
        #[test]
        fn non_standard_strings_dont_crash(ref s in "\\PC*") {
            let result = EntrySigned::new(s);
            assert!(result.is_err())
        }
    }

    proptest! {
        #[test]
        fn partially_correct_strings_dont_crash(
            ref author in "[0-9a-f]{64}|[*]{0}",
            ref log_id in prop::num::u64::ANY,
            ref seq_num in prop::num::u64::ANY,
            ref skiplink in "[0-9a-f]{68}|[*]{0}",
            ref backlink in "[0-9a-f]{68}|[*]{0}",
            ref payload_size in prop::num::u64::ANY,
            ref payload_hash in "[0-9a-f]{68}|[*]{0}",
            ref signature in "[0-9a-f]{68}|[*]{0}"
        ) {
            let encoded_entry = format!("0{}{}{}{}{}{}{}{}",
                author.as_str(),
                log_id,
                seq_num,
                skiplink.as_str(),
                backlink.as_str(),
                payload_size,
                payload_hash.as_str(),
                signature,
            );
            let result = EntrySigned::new(&encoded_entry);
            assert!(result.is_err())
        }
    }
} */
