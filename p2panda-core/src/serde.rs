// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;
use std::marker::PhantomData;

use serde::de::{Error as SerdeError, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteBuf as SerdeByteBuf, Bytes as SerdeBytes};

use crate::hash::{Hash, HashError};
use crate::identity::{IdentityError, PrivateKey, PublicKey, Signature};
use crate::operation::{Body, Header};

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
        seq.serialize_element(&self.public_key)?;

        if let Some(signature) = &self.signature {
            seq.serialize_element(signature)?;
        }

        seq.serialize_element(&self.payload_size)?;
        if let Some(hash) = &self.payload_hash {
            seq.serialize_element(&hash)?;
        }

        seq.serialize_element(&self.timestamp)?;
        seq.serialize_element(&self.seq_num)?;

        if let Some(backlink) = &self.backlink {
            seq.serialize_element(backlink)?;
        }

        seq.serialize_element(&self.previous)?;

        // @TODO: there is an opportunity to skip serializing if `E` is a zero-sized type,
        // and save one byte.
        seq.serialize_element(&self.extensions)?;

        seq.end()
    }
}

impl<'de, E> Deserialize<'de> for Header<E>
where
    E: Deserialize<'de>,
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
            E: Deserialize<'de>,
        {
            type Value = Header<E>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("Header encoded as a sequence")
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

                // @TODO: If `E` is a zero-sized type, use `mem::conjure_zst` when ready.
                // See https://github.com/rust-lang/rust/pull/146479
                let extensions: E = seq
                    .next_element()
                    .map_err(|err| SerdeError::custom(format!("invalid extensions: {err}")))?
                    .ok_or(SerdeError::custom("extensions missing"))?;

                Ok(Header {
                    version,
                    public_key,
                    signature: Some(signature),
                    payload_hash,
                    payload_size,
                    timestamp,
                    seq_num,
                    backlink,
                    previous,
                    extensions,
                })
            }
        }

        deserializer.deserialize_seq(HeaderVisitor::<E> {
            _marker: PhantomData,
        })
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

    use crate::Body;
    use crate::hash::Hash;
    use crate::identity::{PrivateKey, PublicKey};
    use crate::operation::Header;

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
        mut header: Header<E>,
        private_key: &PrivateKey,
    ) {
        header.sign(private_key);

        let mut bytes = Vec::new();
        ciborium::ser::into_writer(&header, &mut bytes).unwrap();
        let header_again: Header<E> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(header, header_again);
    }

    #[test]
    fn serde_roundtrip_operations() {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        struct CustomExtensions {
            custom_field: u64,
        }

        let extensions = CustomExtensions { custom_field: 12 };
        let private_key = PrivateKey::new();

        assert_serde_roundtrip(
            Header::<CustomExtensions> {
                version: 1,
                public_key: private_key.public_key(),
                payload_size: 123,
                payload_hash: Some(Hash::new(vec![1, 2, 3])),
                timestamp: 0,
                seq_num: 0,
                backlink: None,
                previous: vec![],
                extensions: extensions.clone(),
                signature: None,
            },
            &private_key,
        );

        assert_serde_roundtrip(
            Header::<CustomExtensions> {
                version: 1,
                public_key: private_key.public_key(),
                payload_size: 0,
                payload_hash: None,
                timestamp: 0,
                seq_num: 0,
                backlink: None,
                previous: vec![],
                extensions: extensions,
                signature: None,
            },
            &private_key,
        );
    }

    #[test]
    fn expected_de_error() {
        let private_key = PrivateKey::new();

        // payload size given without payload hash
        let mut header = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: 2829099,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: (),
        };
        header.sign(&private_key);

        let result = ciborium::de::from_reader::<Header<()>, _>(&header.to_bytes()[..]);
        assert!(result.is_err());

        // payload hash given without payload size
        let mut header = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: 0,
            payload_hash: Some(Hash::new([0, 1, 2])),
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: (),
        };
        header.sign(&private_key);

        let result = ciborium::de::from_reader::<Header<()>, _>(&header.to_bytes()[..]);
        assert!(result.is_err());

        // backlink given with seq number 0
        let mut header = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: Some(Hash::new([0, 1, 2])),
            previous: vec![],
            extensions: (),
        };
        header.sign(&private_key);

        let result = ciborium::de::from_reader::<Header<()>, _>(&header.to_bytes()[..]);
        assert!(result.is_err());

        // backlink not given with seq number > 0
        let mut header = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 10,
            backlink: None,
            previous: vec![],
            extensions: (),
        };
        header.sign(&private_key);

        let result = ciborium::de::from_reader::<Header<()>, _>(&header.to_bytes()[..]);
        assert!(result.is_err());
    }

    #[test]
    fn serde_header_with_other_types() {
        let private_key = PrivateKey::new();

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Message {
            header: Header<()>,
            body: Body,
        }

        let body = Body::new(b"hello");
        let mut header = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: (),
        };
        header.sign(&private_key);

        let message = Message { header, body };

        let mut bytes = Vec::new();
        ciborium::ser::into_writer(&message, &mut bytes).unwrap();

        let message_again: Message = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(message_again, message);
    }

    #[test]
    fn fixtures() {
        let private_key = PrivateKey::from_bytes(&[
            244, 123, 85, 215, 161, 204, 94, 227, 239, 253, 128, 164, 228, 160, 195, 49, 18, 49,
            125, 4, 50, 218, 157, 230, 174, 1, 154, 231, 231, 142, 22, 170,
        ]);

        // header at seq num 0 with no previous
        let mut header_0 = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: (),
        };
        header_0.sign(&private_key);

        let bytes = [
            136, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 177,
            60, 248, 186, 240, 172, 58, 52, 236, 91, 174, 35, 231, 179, 180, 2, 105, 53, 7, 78, 71,
            179, 99, 159, 171, 47, 250, 15, 242, 228, 75, 39, 112, 204, 138, 63, 88, 171, 227, 239,
            194, 88, 171, 32, 138, 26, 93, 203, 190, 178, 85, 186, 245, 227, 45, 65, 169, 195, 90,
            212, 39, 49, 28, 0, 0, 0, 0, 128, 246,
        ];

        let header_again: Header<()> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(header_0, header_again);

        // header at seq num 0 with previous
        let mut header_0_with_previous = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![header_0.hash()],
            extensions: (),
        };
        header_0_with_previous.sign(&private_key);

        let bytes = [
            136, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 86,
            187, 206, 107, 141, 200, 7, 157, 107, 47, 49, 47, 4, 177, 76, 141, 51, 230, 245, 151,
            124, 185, 157, 79, 59, 59, 14, 91, 105, 6, 229, 145, 62, 167, 203, 221, 253, 28, 128,
            113, 59, 30, 148, 3, 153, 7, 16, 55, 52, 195, 216, 9, 97, 167, 205, 26, 141, 82, 229,
            39, 124, 198, 186, 9, 0, 0, 0, 129, 88, 32, 201, 88, 182, 128, 125, 179, 108, 4, 23,
            151, 168, 52, 216, 181, 123, 95, 11, 78, 75, 6, 236, 167, 141, 219, 243, 115, 255, 203,
            100, 128, 30, 92, 246,
        ];

        let header_again: Header<()> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(header_0_with_previous, header_again);

        // header at seq num 0 with previous and body
        let body = Body::new("Hello, Sloth!".as_bytes());
        let mut header_0_with_previous_and_body = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![header_0.hash()],
            extensions: (),
        };
        header_0_with_previous_and_body.sign(&private_key);

        let bytes = [
            137, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 152,
            61, 237, 59, 97, 221, 165, 207, 164, 49, 55, 177, 168, 40, 36, 190, 47, 59, 86, 231,
            231, 117, 182, 186, 45, 142, 7, 98, 17, 1, 153, 173, 165, 127, 208, 222, 173, 157, 81,
            165, 228, 184, 230, 165, 88, 173, 41, 104, 171, 12, 18, 57, 179, 18, 254, 50, 65, 226,
            147, 228, 201, 28, 123, 7, 13, 88, 32, 191, 127, 68, 13, 227, 43, 252, 155, 49, 148,
            176, 2, 162, 217, 175, 171, 49, 44, 181, 215, 71, 113, 211, 195, 29, 128, 192, 169, 5,
            138, 160, 142, 0, 0, 129, 88, 32, 201, 88, 182, 128, 125, 179, 108, 4, 23, 151, 168,
            52, 216, 181, 123, 95, 11, 78, 75, 6, 236, 167, 141, 219, 243, 115, 255, 203, 100, 128,
            30, 92, 246,
        ];

        let header_again: Header<()> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(header_0_with_previous_and_body, header_again);

        // header at seq num 1 with backlink but no previous
        let mut header_1 = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 1,
            backlink: Some(header_0.hash()),
            previous: vec![],
            extensions: (),
        };
        header_1.sign(&private_key);

        let bytes = [
            137, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 159,
            29, 188, 12, 62, 68, 90, 135, 233, 157, 134, 251, 205, 180, 206, 144, 230, 240, 161,
            42, 236, 217, 77, 210, 161, 229, 92, 67, 213, 170, 105, 53, 16, 57, 43, 62, 143, 162,
            177, 91, 154, 154, 131, 45, 138, 152, 49, 80, 42, 144, 249, 30, 166, 143, 139, 255, 54,
            65, 180, 15, 102, 222, 210, 6, 0, 0, 1, 88, 32, 201, 88, 182, 128, 125, 179, 108, 4,
            23, 151, 168, 52, 216, 181, 123, 95, 11, 78, 75, 6, 236, 167, 141, 219, 243, 115, 255,
            203, 100, 128, 30, 92, 128, 246,
        ];

        let header_again: Header<()> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(header_1, header_again);

        // header at seq num 1 with previous
        let mut header_1_with_previous = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 1,
            backlink: Some(header_0.hash()),
            previous: vec![header_0.hash()],
            extensions: (),
        };
        header_1_with_previous.sign(&private_key);

        let bytes = [
            137, 1, 88, 32, 228, 21, 196, 25, 12, 199, 241, 100, 122, 89, 46, 191, 142, 95, 144,
            92, 42, 222, 249, 148, 139, 23, 91, 43, 92, 17, 225, 69, 17, 181, 22, 32, 88, 64, 147,
            144, 109, 231, 188, 44, 47, 38, 189, 192, 85, 151, 242, 49, 40, 30, 155, 198, 216, 52,
            140, 216, 65, 66, 19, 227, 159, 175, 23, 107, 113, 180, 100, 44, 161, 228, 126, 219,
            10, 85, 71, 59, 156, 117, 23, 1, 101, 224, 96, 75, 45, 25, 17, 37, 56, 78, 184, 120,
            39, 115, 95, 127, 83, 3, 0, 0, 1, 88, 32, 201, 88, 182, 128, 125, 179, 108, 4, 23, 151,
            168, 52, 216, 181, 123, 95, 11, 78, 75, 6, 236, 167, 141, 219, 243, 115, 255, 203, 100,
            128, 30, 92, 129, 88, 32, 201, 88, 182, 128, 125, 179, 108, 4, 23, 151, 168, 52, 216,
            181, 123, 95, 11, 78, 75, 6, 236, 167, 141, 219, 243, 115, 255, 203, 100, 128, 30, 92,
            246,
        ];

        let header_again: Header<()> = ciborium::de::from_reader(&bytes[..]).unwrap();
        assert_eq!(header_1_with_previous, header_again);
    }

    #[test]
    fn decode_non_map_extensions() {
        let private_key = PrivateKey::new();

        let mut header = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: (),
        };
        header.sign(&private_key);

        let result = ciborium::de::from_reader::<Header<()>, _>(&header.to_bytes()[..]);
        assert!(result.is_ok());
    }
}
