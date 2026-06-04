// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;
use std::marker::PhantomData;

use serde::de::{Error as SerdeError, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteBuf as SerdeByteBuf, Bytes as SerdeBytes};

use crate::cursor::Cursor;
use crate::hash::{Hash, HashError};
use crate::identity::{Author, IdentityError, Signature, SigningKey, VerifyingKey};
use crate::logs::{LogHeights, LogId};
use crate::operation::{Body, Header};
use crate::topic::{Topic, TopicError};

/// Helper method for `serde` to serialize bytes into a hex string when using a human readable
/// encoding (JSON, GraphQL), otherwise it serializes the bytes directly (CBOR).
pub fn serialize_hex<S>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if serializer.is_human_readable() {
        hex::serde::serialize(value, serializer)
    } else {
        SerdeBytes::new(value).serialize(serializer)
    }
}

/// Helper method for `serde` to deserialize from a hex string into bytes when using a human
/// readable encoding (JSON, GraphQL), otherwise it deserializes the bytes directly (CBOR).
pub fn deserialize_hex<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    if deserializer.is_human_readable() {
        hex::serde::deserialize(deserializer)
    } else {
        let bytes = <SerdeByteBuf>::deserialize(deserializer)?;
        Ok(bytes.to_vec())
    }
}

impl Serialize for Hash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_hex(self.as_bytes(), serializer)
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserialize_hex(deserializer)?;

        bytes
            .as_slice()
            .try_into()
            .map_err(|err: HashError| serde::de::Error::custom(err.to_string()))
    }
}

impl Serialize for SigningKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_hex(self.as_bytes(), serializer)
    }
}

impl<'de> Deserialize<'de> for SigningKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserialize_hex(deserializer)?;

        bytes
            .as_slice()
            .try_into()
            .map_err(|err: IdentityError| serde::de::Error::custom(err.to_string()))
    }
}

impl Serialize for VerifyingKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_hex(self.as_bytes(), serializer)
    }
}

impl<'de> Deserialize<'de> for VerifyingKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserialize_hex(deserializer)?;

        bytes
            .as_slice()
            .try_into()
            .map_err(|err: IdentityError| serde::de::Error::custom(err.to_string()))
    }
}

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_hex(&self.to_bytes(), serializer)
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserialize_hex(deserializer)?;

        bytes
            .as_slice()
            .try_into()
            .map_err(|err: IdentityError| serde::de::Error::custom(err.to_string()))
    }
}

impl<E> Serialize for Header<E>
where
    E: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.field_count()))?;
        seq.serialize_element(&self.version)?;
        seq.serialize_element(&self.verifying_key)?;
        seq.serialize_element(&self.signature)?;
        seq.serialize_element(&self.payload_size)?;

        if let Some(hash) = &self.payload_hash {
            seq.serialize_element(&hash)?;
        }

        seq.serialize_element(&self.seq_num)?;

        if let Some(backlink) = &self.backlink {
            seq.serialize_element(backlink)?;
        }

        if Self::has_non_zero_sized_extensions() {
            seq.serialize_element(&self.extensions)?;
        }

        seq.end()
    }
}

// impl<'de, E> Deserialize<'de> for Header<E>
// where
//     E: Deserialize<'de>,
// {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: serde::Deserializer<'de>,
//     {
//         struct HeaderVisitor<E> {
//             _marker: PhantomData<E>,
//         }
//
//         impl<'de, E> Visitor<'de> for HeaderVisitor<E>
//         where
//             E: Deserialize<'de>,
//         {
//             type Value = Header<E>;
//
//             fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//                 formatter.write_str("Header encoded as a sequence")
//             }
//
//             fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
//             where
//                 A: SeqAccess<'de>,
//             {
//                 let version: Version = seq
//                     .next_element()?
//                     .ok_or(SerdeError::custom("version missing"))?;
//
//                 let verifying_key: VerifyingKey = seq
//                     .next_element()?
//                     .ok_or(SerdeError::custom("public key missing"))?;
//
//                 let signature: Signature = seq
//                     .next_element()?
//                     .ok_or(SerdeError::custom("signature missing"))?;
//
//                 let payload_size: u32 = seq
//                     .next_element()?
//                     .ok_or(SerdeError::custom("payload size missing"))?;
//
//                 let payload_hash: Option<Hash> = match payload_size {
//                     0 => None,
//                     _ => {
//                         let hash: Hash = seq
//                             .next_element()?
//                             .ok_or(SerdeError::custom("payload hash missing"))?;
//                         Some(hash)
//                     }
//                 };
//
//                 let seq_num: SeqNum = seq
//                     .next_element()?
//                     .ok_or(SerdeError::custom("sequence number missing"))?;
//
//                 let backlink: Option<Hash> = match seq_num {
//                     0 => None,
//                     _ => {
//                         let hash: Hash = seq
//                             .next_element()?
//                             .ok_or(SerdeError::custom("backlink missing"))?;
//                         Some(hash)
//                     }
//                 };
//
//                 let extensions: E = if Header::<E>::has_non_zero_sized_extensions() {
//                     seq.next_element()?
//                         .ok_or(SerdeError::custom("extensions missing"))?
//                 } else {
//                     Header::<E>::zero_sized_extensions()
//                 };
//
//                 if let Some(remainder) = seq.size_hint()
//                     && remainder > 0
//                 {
//                     return Err(SerdeError::custom("unexpected excessive fields in header"));
//                 }
//
//                 Ok(Header {
//                     version,
//                     verifying_key,
//                     signature: Some(signature),
//                     payload_hash,
//                     payload_size,
//                     seq_num,
//                     backlink,
//                     extensions,
//                 })
//             }
//         }
//
//         deserializer.deserialize_seq(HeaderVisitor::<E> {
//             _marker: PhantomData,
//         })
//     }
// }

impl Serialize for Body {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_hex(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for Body {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserialize_hex(deserializer)?;
        Ok(Body(bytes.to_vec()))
    }
}

impl Serialize for Topic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_hex(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for Topic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserialize_hex(deserializer)?;

        bytes
            .as_slice()
            .try_into()
            .map_err(|err: TopicError| serde::de::Error::custom(err.to_string()))
    }
}

impl<A, L> Serialize for Cursor<A, L>
where
    A: Author,
    L: LogId,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element(self.name())?;
        seq.serialize_element(self.state())?;
        seq.end()
    }
}

impl<'de, A, L> Deserialize<'de> for Cursor<A, L>
where
    A: Author,
    L: LogId,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct CursorVisitor<A, L> {
            _marker: PhantomData<(A, L)>,
        }

        impl<'de, A, L> Visitor<'de> for CursorVisitor<A, L>
        where
            A: Author,
            L: LogId,
        {
            type Value = Cursor<A, L>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("Cursor encoded as a sequence")
            }

            fn visit_seq<T>(self, mut seq: T) -> Result<Self::Value, T::Error>
            where
                T: SeqAccess<'de>,
            {
                let name: String = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("cursor id missing"))?;

                let state: LogHeights<A, L> = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("state vector missing"))?;

                Ok(Cursor::new(name, state))
            }
        }

        deserializer.deserialize_seq(CursorVisitor::<A, L> {
            _marker: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use crate::Extensions;
    use crate::cbor::{decode_cbor, encode_cbor};
    use crate::hash::Hash;
    use crate::identity::{SigningKey, VerifyingKey};
    use crate::operation::{AnyHeader, Header};

    use super::{deserialize_hex, serialize_hex};

    #[derive(Debug, Serialize, Deserialize)]
    struct Test(
        #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
    );

    #[test]
    fn serialize() {
        let test = Test(vec![1, 2, 3]);

        // For CBOR the bytes just get serialized straight away as it is not a human readable
        // encoding.
        let bytes = encode_cbor(&test).unwrap();
        assert_eq!(vec![67, 1, 2, 3], bytes);
    }

    #[test]
    fn deserialize() {
        let bytes: Vec<u8> = vec![67, 1, 2, 3];

        // For CBOR the bytes just get deserialized straight away as an array as it is not a human
        // readable encoding
        let test: Test = decode_cbor(&bytes[..]).unwrap();
        assert_eq!(test.0, vec![1, 2, 3]);
    }

    #[test]
    fn serialize_hash() {
        // Serialize CBOR (non human-readable byte encoding)
        let hash = Hash::digest([1, 2, 3]);
        let bytes = encode_cbor(&hash).unwrap();
        assert_eq!(
            bytes,
            vec![
                88, 32, 177, 119, 236, 27, 242, 109, 251, 59, 112, 16, 212, 115, 230, 212, 71, 19,
                178, 155, 118, 91, 153, 198, 230, 14, 203, 250, 231, 66, 222, 73, 101, 67
            ]
        );

        // Serialize JSON (human-readable hex encoding)
        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(
            json,
            "\"b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543\""
        );
    }

    #[test]
    fn deserialize_hash() {
        // Deserialize CBOR (non human-readable byte encoding)
        let bytes = [
            88, 32, 177, 119, 236, 27, 242, 109, 251, 59, 112, 16, 212, 115, 230, 212, 71, 19, 178,
            155, 118, 91, 153, 198, 230, 14, 203, 250, 231, 66, 222, 73, 101, 67,
        ];
        let hash: Hash = decode_cbor(&bytes[..]).unwrap();
        assert_eq!(hash, Hash::digest([1, 2, 3]));

        // Deserialize JSON (human-readable hex encoding)
        let json = "\"b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543\"";
        let hash: Hash = serde_json::from_str(json).unwrap();
        assert_eq!(hash, Hash::digest([1, 2, 3]));
    }

    #[test]
    fn serialize_verifying_key() {
        // Serialize CBOR (non human-readable byte encoding)
        let verifying_key = VerifyingKey::from_bytes(&[
            215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14, 225, 114,
            243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26,
        ])
        .unwrap();
        let bytes = encode_cbor(&verifying_key).unwrap();
        assert_eq!(
            bytes,
            vec![
                88, 32, 215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14,
                225, 114, 243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26,
            ]
        );

        // Serialize JSON (human-readable hex encoding)
        let json = serde_json::to_string(&verifying_key).unwrap();
        assert_eq!(
            json,
            "\"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a\""
        );
    }

    fn assert_serde_roundtrip<E>(header: Header<E>)
    where
        E: Extensions + PartialEq,
    {
        let bytes = header.encode();
        let any_header = AnyHeader::decode(&bytes).expect("valid header");
        let header_again: Header<E> = any_header.try_into().expect("valid extensions");

        assert_eq!(header, header_again);
    }

    #[test]
    fn serde_roundtrip_operations() {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        struct CustomExtensions {
            custom_field: u64,
        }

        let extensions = CustomExtensions { custom_field: 12 };
        let signing_key = SigningKey::generate();

        assert_serde_roundtrip(
            Header::builder()
                .body(b"test")
                .build(&signing_key, extensions),
        );
        assert_serde_roundtrip(Header::builder().build(&signing_key, ()));
    }

    #[test]
    fn fixtures() {
        let signing_key = SigningKey::from([
            244, 123, 85, 215, 161, 204, 94, 227, 239, 253, 128, 164, 228, 160, 195, 49, 18, 49,
            125, 4, 50, 218, 157, 230, 174, 1, 154, 231, 231, 142, 22, 170,
        ]);

        // header at seq num 0
        let header = Header::builder().build(&signing_key, ());

        let bytes = vec![
            133, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 17,
            129, 90, 32, 212, 224, 74, 141, 219, 82, 160, 35, 19, 205, 82, 55, 247, 204, 121, 153,
            128, 203, 123, 102, 108, 90, 60, 23, 223, 176, 251, 154, 243, 131, 177, 54, 142, 210,
            0, 231, 125, 90, 206, 28, 240, 37, 179, 88, 200, 246, 185, 49, 246, 135, 242, 133, 128,
            127, 22, 118, 23, 102, 22, 2, 0, 0,
        ];

        assert_eq!(bytes, header.encode());

        let any_header: AnyHeader = bytes.try_into().expect("valid header");
        let header_again: Header<()> = any_header.try_into().expect("valid extensions");
        assert_eq!(header, header_again);

        // header at seq num 0 with body
        let header = Header::builder()
            .body(b"Hello, Sloth!")
            .build(&signing_key, ());

        let bytes = vec![
            134, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 187,
            89, 157, 165, 197, 22, 79, 145, 227, 116, 226, 203, 231, 213, 225, 253, 197, 253, 240,
            147, 16, 224, 186, 146, 94, 126, 79, 185, 150, 84, 102, 16, 109, 56, 241, 228, 164,
            191, 153, 47, 142, 189, 12, 71, 159, 143, 81, 204, 108, 124, 22, 39, 222, 122, 88, 198,
            123, 125, 2, 211, 28, 196, 90, 0, 13, 88, 32, 191, 127, 68, 13, 227, 43, 252, 155, 49,
            148, 176, 2, 162, 217, 175, 171, 49, 44, 181, 215, 71, 113, 211, 195, 29, 128, 192,
            169, 5, 138, 160, 142, 0,
        ];

        assert_eq!(bytes, header.encode());

        let any_header: AnyHeader = bytes.try_into().expect("valid header");
        let header_again: Header<()> = any_header.try_into().expect("valid extensions");
        assert_eq!(header, header_again);

        // header at seq num 1 with backlink
        let header = Header::builder()
            .chain(1, header.hash())
            .build(&signing_key, ());

        let bytes = vec![
            134, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 90,
            241, 219, 179, 113, 96, 207, 245, 193, 3, 115, 166, 84, 177, 236, 191, 194, 134, 34,
            214, 117, 182, 130, 121, 97, 9, 110, 170, 35, 44, 155, 205, 147, 180, 234, 188, 17, 39,
            109, 146, 142, 68, 181, 186, 119, 197, 71, 45, 245, 246, 32, 139, 46, 197, 150, 12,
            255, 110, 134, 99, 5, 139, 223, 13, 0, 1, 88, 32, 68, 43, 250, 251, 47, 151, 121, 58,
            30, 144, 24, 129, 171, 35, 89, 56, 161, 112, 75, 91, 168, 201, 195, 121, 169, 155, 85,
            104, 129, 60, 141, 161,
        ];

        let any_header: AnyHeader = bytes.try_into().expect("valid header");
        let header_again: Header<()> = any_header.try_into().expect("valid extensions");
        assert_eq!(header, header_again);
    }

    #[test]
    fn unexpected_eof_when_incomplete() {
        // The CBOR decoder should be able to detect an "Unexpected EOF" error if we're giving it an
        // incomplete header.
        let incomplete = [
            137, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
        ];

        let result = AnyHeader::decode(&incomplete);
        assert!(result.is_err());
    }

    #[test]
    fn zero_sized_extensions() {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        struct Zilch;

        #[derive(Clone, Debug, Serialize, Deserialize)]
        struct ZeroSizedExtension {
            field_a: [u8; 0],
            field_b: (),
            field_c: Zilch,
        }

        let signing_key = SigningKey::generate();

        let header = Header::builder().body(b"look, no bytes!").build(
            &signing_key,
            ZeroSizedExtension {
                field_a: [],
                field_b: (),
                field_c: Zilch,
            },
        );

        let bytes = header.encode();

        // Make sure we skip the extensions field which means we only need 6 fields for the header.
        //
        // In CBOR this shows in the first byte where the "array" type + its length is declared
        // (array(6)). In hex this would be represented by `86`, in decimal its `134`:
        assert!(bytes[0] == 134);

        // We correctly deserialize to the ZST.
        let any_header: AnyHeader = bytes.try_into().expect("valid header");
        let result: Header<ZeroSizedExtension> = any_header.try_into().expect("valid extensions");
        assert_eq!(result.extensions.field_a.len(), 0);
        assert_eq!(result.extensions.field_b, ());
        assert_eq!(result.extensions.field_c, Zilch);
    }
}
