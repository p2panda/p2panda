// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::marker::PhantomData;

use serde::de::{DeserializeOwned, Error as SerdeError, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteBuf as SerdeByteBuf, Bytes as SerdeBytes};

use crate::hash::{Hash, HashError};
use crate::identity::{IdentityError, PrivateKey, PublicKey, Signature};
use crate::operation::{Body, Header, UnsignedHeader};

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

pub fn serialize_header<S, E>(
    serializer: S,
    header: &UnsignedHeader<E>,
    signature: Option<&Signature>,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    E: Clone + Serialize + DeserializeOwned,
{
    let mut seq = serializer.serialize_seq(None)?;

    seq.serialize_element(&header.version)?;
    seq.serialize_element(&header.public_key)?;
    if let Some(signature) = signature {
        seq.serialize_element(signature)?;
    }

    seq.serialize_element(&header.payload_size)?;
    if let Some(hash) = &header.payload_hash {
        seq.serialize_element(&hash)?;
    }

    seq.serialize_element(&header.timestamp)?;
    seq.serialize_element(&header.seq_num)?;

    if let Some(backlink) = &header.backlink {
        seq.serialize_element(backlink)?;
    }

    seq.serialize_element(&header.previous)?;

    if let Some(extension) = &header.extension {
        seq.serialize_element(extension)?;
    }

    seq.end()
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

impl Serialize for PrivateKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_hex(self.as_bytes(), serializer)
    }
}

impl<'de> Deserialize<'de> for PrivateKey {
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

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_hex(self.as_bytes(), serializer)
    }
}

impl<'de> Deserialize<'de> for PublicKey {
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

impl<E> Serialize for UnsignedHeader<E>
where
    E: Clone + Serialize + DeserializeOwned,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_header(serializer, self, None)
    }
}

impl<'de, E> Deserialize<'de> for Header<E>
where
    E: Clone + Serialize + DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HeaderVisitor<E> {
            _marker: PhantomData<E>,
        }

        impl<'de, E> Visitor<'de> for HeaderVisitor<E>
        where
            E: Clone + Serialize + DeserializeOwned,
        {
            type Value = Header<E>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("UnsignedHeader encoded as a sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let version: u64 = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid version, expected u64"))?
                    .ok_or(SerdeError::custom("version missing"))?;

                let public_key: PublicKey = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid public key, expected bytes"))?
                    .ok_or(SerdeError::custom("public key missing"))?;

                let signature: Signature = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid signature, expected bytes"))?
                    .ok_or(SerdeError::custom("signature missing"))?;

                let payload_size: u64 = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid payload size, expected u64"))?
                    .ok_or(SerdeError::custom("payload size missing"))?;

                let payload_hash: Option<Hash> = match payload_size {
                    0 => None,
                    _ => {
                        let hash: Hash = seq
                            .next_element()
                            .map_err(|_| {
                                SerdeError::custom("invalid payload hash, expected bytes")
                            })?
                            .ok_or(SerdeError::custom("payload hash missing"))?;
                        Some(hash)
                    }
                };

                let timestamp: u64 = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid timestamp, expected u64"))?
                    .ok_or(SerdeError::custom("timestamp missing"))?;

                let seq_num: u64 = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid sequence number, expected u64"))?
                    .ok_or(SerdeError::custom("sequence number missing"))?;

                let backlink: Option<Hash> = match seq_num {
                    0 => None,
                    _ => {
                        let hash: Hash = seq
                            .next_element()
                            .map_err(|err| {
                                SerdeError::custom(format!(
                                    "invalid backlink, expected bytes {err}"
                                ))
                            })?
                            .ok_or(SerdeError::custom("backlink missing"))?;
                        Some(hash)
                    }
                };

                let previous: Vec<Hash> = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid previous links, expected array"))?
                    .ok_or(SerdeError::custom("previous array missing"))?;

                let extension: Option<E> = seq
                    .next_element()
                    .map_err(|err| SerdeError::custom(format!("invalid extension: {err}")))?;

                let unsigned_header = UnsignedHeader {
                    version,
                    public_key,
                    payload_hash,
                    payload_size,
                    timestamp,
                    seq_num,
                    backlink,
                    previous,
                    extension,
                };

                Ok(Header(signature, unsigned_header))
            }
        }

        deserializer.deserialize_seq(HeaderVisitor::<E> {
            _marker: PhantomData,
        })
    }
}

impl<E> Serialize for Header<E>
where
    E: Clone + Serialize + DeserializeOwned,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_header(serializer, &self.1, Some(&self.0))
    }
}

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

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Serialize};

    use crate::hash::Hash;
    use crate::identity::{PrivateKey, PublicKey};
    use crate::operation::{Header, UnsignedHeader};
    use crate::Body;

    use super::{deserialize_hex, serialize_hex};

    #[derive(Debug, Serialize, Deserialize)]
    struct Test(
        #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
    );

    #[test]
    fn serialize() {
        let mut bytes: Vec<u8> = Vec::new();
        let test = Test(vec![1, 2, 3]);

        // For CBOR the bytes just get serialized straight away as it is not a human readable
        // encoding
        ciborium::ser::into_writer(&test, &mut bytes).unwrap();
        assert_eq!(vec![67, 1, 2, 3], bytes);
    }

    #[test]
    fn deserialize() {
        let bytes: Vec<u8> = vec![67, 1, 2, 3];

        // For CBOR the bytes just get deserialized straight away as an array as it is not a human
        // readable encoding
        let test: Test = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(test.0, vec![1, 2, 3]);
    }

    #[test]
    fn serialize_hash() {
        // Serialize CBOR (non human-readable byte encoding)
        let mut bytes: Vec<u8> = Vec::new();
        let hash = Hash::new([1, 2, 3]);
        ciborium::ser::into_writer(&hash, &mut bytes).unwrap();
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
        let hash: Hash = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(hash, Hash::new([1, 2, 3]));

        // Deserialize JSON (human-readable hex encoding)
        let json = "\"b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543\"";
        let hash: Hash = serde_json::from_str(json).unwrap();
        assert_eq!(hash, Hash::new([1, 2, 3]));
    }

    #[test]
    fn serialize_public_key() {
        // Serialize CBOR (non human-readable byte encoding)
        let mut bytes: Vec<u8> = Vec::new();
        let public_key = PublicKey::from_bytes(&[
            215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14, 225, 114,
            243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26,
        ])
        .unwrap();
        ciborium::ser::into_writer(&public_key, &mut bytes).unwrap();
        assert_eq!(
            bytes,
            vec![
                88, 32, 215, 90, 152, 1, 130, 177, 10, 183, 213, 75, 254, 211, 201, 100, 7, 58, 14,
                225, 114, 243, 218, 166, 35, 37, 175, 2, 26, 104, 247, 7, 81, 26,
            ]
        );

        // Serialize JSON (human-readable hex encoding)
        let json = serde_json::to_string(&public_key).unwrap();
        assert_eq!(
            json,
            "\"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a\""
        );
    }

    fn assert_serde_roundtrip<
        E: Clone + std::fmt::Debug + PartialEq + Serialize + DeserializeOwned,
    >(
        header: UnsignedHeader<E>,
        private_key: &PrivateKey,
    ) {
        let header = header;
        let signed_header = header.sign(&private_key);

        let mut bytes = Vec::new();
        ciborium::ser::into_writer(&signed_header, &mut bytes).unwrap();
        let signed_header_again: Header<E> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(signed_header, signed_header_again);
    }

    #[test]
    fn serde_roundtrip_operations() {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        struct Extension {
            custom_field: u64,
        }

        let extension = Extension { custom_field: 12 };
        let private_key = PrivateKey::new();

        assert_serde_roundtrip(
            UnsignedHeader::<Extension> {
                version: 1,
                public_key: private_key.public_key(),
                payload_size: 123,
                payload_hash: Some(Hash::new(vec![1, 2, 3])),
                timestamp: 0,
                seq_num: 0,
                backlink: None,
                previous: vec![],
                extension: Some(extension.clone()),
            },
            &private_key,
        );

        assert_serde_roundtrip(
            UnsignedHeader::<Extension> {
                version: 1,
                public_key: private_key.public_key(),
                payload_size: 0,
                payload_hash: None,
                timestamp: 0,
                seq_num: 7,
                backlink: Some(Hash::new(vec![1, 2, 3])),
                previous: vec![],
                extension: None,
            },
            &private_key,
        );

        assert_serde_roundtrip(
            UnsignedHeader::<Extension> {
                version: 1,
                public_key: private_key.public_key(),
                payload_size: 0,
                payload_hash: None,
                timestamp: 0,
                seq_num: 0,
                backlink: None,
                previous: vec![],
                extension: Some(extension),
            },
            &private_key,
        );
    }

    #[test]
    fn expected_de_error() {
        let private_key = PrivateKey::new();

        // payload size given without payload hash
        let header = UnsignedHeader::<()> {
            version: 1,
            public_key: private_key.public_key(),
            payload_size: 2829099,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extension: None,
        }
        .sign(&private_key);

        let result = ciborium::de::from_reader::<Header<()>, _>(&header.to_bytes()[..]);
        assert!(result.is_err());

        // payload hash given without payload size
        let header = UnsignedHeader::<()> {
            version: 1,
            public_key: private_key.public_key(),
            payload_size: 0,
            payload_hash: Some(Hash::new([0, 1, 2])),
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extension: None,
        }
        .sign(&private_key);

        let result = ciborium::de::from_reader::<Header<()>, _>(&header.to_bytes()[..]);
        assert!(result.is_err());

        // backlink given with seq number 0
        let header = UnsignedHeader::<()> {
            version: 1,
            public_key: private_key.public_key(),
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: Some(Hash::new([0, 1, 2])),
            previous: vec![],
            extension: None,
        }
        .sign(&private_key);

        let result = ciborium::de::from_reader::<Header<()>, _>(&header.to_bytes()[..]);
        assert!(result.is_err());

        // backlink not given with seq number > 0
        let header = UnsignedHeader::<()> {
            version: 1,
            public_key: private_key.public_key(),
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 10,
            backlink: None,
            previous: vec![],
            extension: None,
        }
        .sign(&private_key);

        let result = ciborium::de::from_reader::<Header<()>, _>(&header.to_bytes()[..]);
        assert!(result.is_err());
    }

    #[test]
    fn fixtures() {
        let private_key = PrivateKey::from_bytes(&[
            244, 123, 85, 215, 161, 204, 94, 227, 239, 253, 128, 164, 228, 160, 195, 49, 18, 49,
            125, 4, 50, 218, 157, 230, 174, 1, 154, 231, 231, 142, 22, 170,
        ]);

        // header at seq num 0 with no previous
        let header_0 = UnsignedHeader::<()> {
            version: 1,
            public_key: private_key.public_key(),
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extension: None,
        }
        .sign(&private_key);

        let bytes = [
            159, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 151,
            14, 56, 41, 13, 112, 102, 141, 219, 131, 11, 17, 248, 53, 120, 203, 78, 204, 169, 210,
            33, 121, 242, 84, 73, 190, 24, 71, 4, 33, 4, 47, 24, 3, 69, 15, 241, 116, 192, 27, 107,
            131, 197, 49, 27, 41, 167, 116, 131, 215, 33, 86, 197, 109, 158, 152, 174, 240, 109,
            151, 79, 151, 31, 0, 0, 0, 0, 128, 255,
        ];

        let header_again: Header<()> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(header_0, header_again);

        // header at seq num 0 with previous
        let header_0_with_previous = UnsignedHeader::<()> {
            version: 1,
            public_key: private_key.public_key(),
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![header_0.hash()],
            extension: None,
        }
        .sign(&private_key);

        let bytes = [
            159, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 1,
            224, 130, 220, 51, 110, 202, 23, 113, 240, 208, 10, 13, 114, 146, 175, 49, 21, 189,
            139, 33, 129, 21, 104, 162, 60, 69, 31, 195, 207, 200, 250, 37, 220, 70, 143, 86, 50,
            94, 44, 147, 211, 227, 101, 130, 88, 238, 42, 35, 243, 1, 112, 77, 94, 106, 61, 190,
            248, 89, 199, 191, 77, 15, 13, 0, 0, 0, 129, 88, 32, 62, 65, 169, 234, 245, 255, 26,
            96, 213, 117, 30, 218, 58, 168, 139, 214, 41, 102, 11, 1, 177, 148, 177, 198, 247, 206,
            65, 12, 118, 98, 169, 129, 255,
        ];

        let header_again: Header<()> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(header_0_with_previous, header_again);

        // header at seq num 0 with previous and body
        let body = Body::new("Hello, Sloth!".as_bytes());
        let header_0_with_previous_and_body = UnsignedHeader::<()> {
            version: 1,
            public_key: private_key.public_key(),
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![header_0.hash()],
            extension: None,
        }
        .sign(&private_key);

        let bytes = [
            159, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 144,
            8, 21, 121, 191, 103, 12, 224, 9, 22, 216, 194, 133, 166, 38, 6, 130, 105, 155, 62,
            101, 119, 220, 71, 92, 255, 88, 216, 247, 109, 119, 99, 25, 232, 207, 85, 242, 185,
            247, 249, 145, 69, 244, 55, 228, 231, 178, 129, 40, 198, 177, 207, 228, 47, 98, 243,
            95, 236, 159, 17, 102, 147, 98, 5, 13, 88, 32, 191, 127, 68, 13, 227, 43, 252, 155, 49,
            148, 176, 2, 162, 217, 175, 171, 49, 44, 181, 215, 71, 113, 211, 195, 29, 128, 192,
            169, 5, 138, 160, 142, 0, 0, 129, 88, 32, 62, 65, 169, 234, 245, 255, 26, 96, 213, 117,
            30, 218, 58, 168, 139, 214, 41, 102, 11, 1, 177, 148, 177, 198, 247, 206, 65, 12, 118,
            98, 169, 129, 255,
        ];

        let header_again: Header<()> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(header_0_with_previous_and_body, header_again);

        // header at seq num 1 with backlink but no previous
        let header_1 = UnsignedHeader::<()> {
            version: 1,
            public_key: private_key.public_key(),
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 1,
            backlink: Some(header_0.hash()),
            previous: vec![],
            extension: None,
        }
        .sign(&private_key);

        let bytes = [
            159, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 45,
            83, 178, 5, 28, 32, 37, 238, 97, 174, 237, 192, 209, 82, 115, 8, 64, 185, 127, 157, 74,
            57, 105, 96, 51, 39, 203, 130, 202, 53, 254, 168, 151, 103, 87, 134, 223, 22, 137, 197,
            254, 97, 234, 73, 203, 180, 212, 133, 4, 221, 75, 81, 86, 231, 183, 45, 12, 225, 143,
            34, 61, 96, 82, 6, 0, 0, 1, 88, 32, 62, 65, 169, 234, 245, 255, 26, 96, 213, 117, 30,
            218, 58, 168, 139, 214, 41, 102, 11, 1, 177, 148, 177, 198, 247, 206, 65, 12, 118, 98,
            169, 129, 128, 255,
        ];

        let header_again: Header<()> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(header_1, header_again);

        // header at seq num 1 with previous
        let header_1_with_previous = UnsignedHeader::<()> {
            version: 1,
            public_key: private_key.public_key(),
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 1,
            backlink: Some(header_0.hash()),
            previous: vec![header_0.hash()],
            extension: None,
        }
        .sign(&private_key);

        let bytes = [
            159, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 46,
            241, 1, 199, 99, 191, 232, 0, 194, 39, 195, 238, 238, 44, 19, 131, 1, 61, 5, 211, 25,
            212, 123, 76, 32, 255, 28, 45, 41, 25, 51, 239, 172, 33, 23, 9, 100, 15, 76, 201, 235,
            254, 188, 144, 131, 54, 254, 15, 188, 20, 173, 176, 197, 97, 43, 222, 28, 69, 234, 233,
            119, 39, 174, 11, 0, 0, 1, 88, 32, 62, 65, 169, 234, 245, 255, 26, 96, 213, 117, 30,
            218, 58, 168, 139, 214, 41, 102, 11, 1, 177, 148, 177, 198, 247, 206, 65, 12, 118, 98,
            169, 129, 129, 88, 32, 62, 65, 169, 234, 245, 255, 26, 96, 213, 117, 30, 218, 58, 168,
            139, 214, 41, 102, 11, 1, 177, 148, 177, 198, 247, 206, 65, 12, 118, 98, 169, 129, 255,
        ];

        let header_again: Header<()> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(header_1_with_previous, header_again);
    }
}
