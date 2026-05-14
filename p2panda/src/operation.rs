// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::hash::Hash as StdHash;

use p2panda_core::hash::{HASH_LEN, Hash};
use p2panda_core::{PruneFlag, Timestamp, Topic};
use serde::de::{Error as SerdeError, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};

pub type Version = u16;

pub type VariantCode = u16;

/// Header type with our system-level extensions.
pub type Header = p2panda_core::Header<Extensions>;

/// Operation type with our system-level extensions.
pub type Operation = p2panda_core::Operation<Extensions>;

/// Versioning for internal extensions format.
pub(crate) const EXTENSIONS_VERSION: Version = 1;

/// Header extensions used in the event processor pipeline to coordinate system-level concerns, for
/// example pruning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extensions {
    version: Version,
    variant: ExtensionsVariantV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionsVariantV1 {
    Basic(BasicExtensions),
    Causal(CausalExtensions),
}

impl ExtensionsVariantV1 {
    /// Unique code to identify each extension variant.
    pub fn code(&self) -> VariantCode {
        match self {
            ExtensionsVariantV1::Basic(_) => BasicExtensions::VARIANT_CODE,
            ExtensionsVariantV1::Causal(_) => CausalExtensions::VARIANT_CODE,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasicExtensions {
    pub log_id: LogId,
    pub timestamp: Timestamp,
    pub prune_flag: PruneFlag,
}

impl BasicExtensions {
    const VARIANT_CODE: VariantCode = 0x00_00;
    const FIELDS_COUNT: usize = 3;
}

#[allow(unused)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalExtensions {
    pub log_id: LogId,
    pub timestamp: Timestamp,
    pub previous: HashSet<Hash>,
}

impl CausalExtensions {
    const VARIANT_CODE: VariantCode = 0x00_01;
    const FIELDS_COUNT: usize = 3;
}

impl Extensions {
    pub fn from_topic(topic: Topic) -> Self {
        Self {
            version: EXTENSIONS_VERSION,
            variant: ExtensionsVariantV1::Basic(BasicExtensions {
                log_id: LogId::from_topic(topic),
                prune_flag: PruneFlag::default(),
                timestamp: Timestamp::now(),
            }),
        }
    }

    pub fn set_prune_flag(mut self, prune_flag: bool) -> Self {
        match self.variant {
            ExtensionsVariantV1::Basic(mut extensions) => {
                extensions.prune_flag = prune_flag.into();
                self.variant = ExtensionsVariantV1::Basic(extensions);
                self
            }
            ExtensionsVariantV1::Causal(_) => {
                // NOTE: We're using the causal variant only as an example placeholder for now, it
                // is not integrated or even complete yet.
                unimplemented!()
            }
        }
    }

    pub fn version(&self) -> Version {
        self.version
    }

    #[allow(unused)]
    pub(crate) fn variant_code(&self) -> VariantCode {
        match &self.variant {
            ExtensionsVariantV1::Basic(_) => BasicExtensions::VARIANT_CODE,
            ExtensionsVariantV1::Causal(_) => CausalExtensions::VARIANT_CODE,
        }
    }

    pub(crate) fn fields_count(&self) -> usize {
        // (version, variant_code, ...)
        let header_field_count = 2;

        let variant_field_count = match &self.variant {
            ExtensionsVariantV1::Basic(_) => BasicExtensions::FIELDS_COUNT,
            ExtensionsVariantV1::Causal(_) => CausalExtensions::FIELDS_COUNT,
        };

        header_field_count + variant_field_count
    }

    pub fn log_id(&self) -> LogId {
        match &self.variant {
            ExtensionsVariantV1::Basic(extensions) => extensions.log_id,
            ExtensionsVariantV1::Causal(extensions) => extensions.log_id,
        }
    }

    pub fn prune_flag(&self) -> PruneFlag {
        match &self.variant {
            ExtensionsVariantV1::Basic(extensions) => extensions.prune_flag,
            ExtensionsVariantV1::Causal(_) => PruneFlag::new(false),
        }
    }

    pub fn timestamp(&self) -> Timestamp {
        match &self.variant {
            ExtensionsVariantV1::Basic(extensions) => extensions.timestamp,
            ExtensionsVariantV1::Causal(extensions) => extensions.timestamp,
        }
    }
}

impl Serialize for Extensions {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // ```plain
        // (
        //    version[u16],
        //    extensions_variant_code[u16],
        //    extensions_variant[..],
        // )
        // ```
        let mut seq = serializer.serialize_seq(Some(self.fields_count()))?;

        seq.serialize_element(&self.version)?;
        seq.serialize_element(&self.variant.code())?;

        match &self.variant {
            ExtensionsVariantV1::Basic(extensions) => {
                // ```plain
                // (
                //     // Extension header
                //     version[u16],
                //     0x00_00[u16],
                //
                //     // Basic extension variant
                //     log_id[32],
                //     timestamp[u64],
                //     prune_flag[bool],
                // )
                // ```
                seq.serialize_element(&extensions.log_id)?;
                seq.serialize_element(&extensions.timestamp)?;
                seq.serialize_element(&extensions.prune_flag)?;
            }
            ExtensionsVariantV1::Causal(extensions) => {
                // ```plain
                // (
                //     // Extension header
                //     version[u16],
                //     0x00_01[u16],
                //
                //     // Causal extension variant
                //     log_id[32],
                //     timestamp[u64],
                //     previous[Vec[32]],
                // )
                // ```
                seq.serialize_element(&extensions.log_id)?;
                seq.serialize_element(&extensions.timestamp)?;
                seq.serialize_element(&extensions.previous)?;
            }
        }

        seq.end()
    }
}

impl<'de> Deserialize<'de> for Extensions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ExtensionsVisitor;

        impl<'de> Visitor<'de> for ExtensionsVisitor {
            type Value = Extensions;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Node API Extensions encoded as a sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let version: Version = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("version missing"))?;

                if version != EXTENSIONS_VERSION {
                    return Err(SerdeError::custom("unsupported extensions version"));
                }

                let variant_code: VariantCode = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("variant code missing"))?;

                let variant = if variant_code == BasicExtensions::VARIANT_CODE {
                    let log_id: LogId = seq
                        .next_element()?
                        .ok_or(SerdeError::custom("log id missing"))?;

                    let timestamp: Timestamp = seq
                        .next_element()?
                        .ok_or(SerdeError::custom("timestamp missing"))?;

                    let prune_flag: PruneFlag = seq
                        .next_element()?
                        .ok_or(SerdeError::custom("prune flag missing"))?;

                    ExtensionsVariantV1::Basic(BasicExtensions {
                        log_id,
                        timestamp,
                        prune_flag,
                    })
                } else if variant_code == CausalExtensions::VARIANT_CODE {
                    let log_id: LogId = seq
                        .next_element()?
                        .ok_or(SerdeError::custom("log id missing"))?;

                    let timestamp: Timestamp = seq
                        .next_element()?
                        .ok_or(SerdeError::custom("timestamp missing"))?;

                    let previous: HashSet<Hash> = seq
                        .next_element()?
                        .ok_or(SerdeError::custom("previous field missing"))?;

                    ExtensionsVariantV1::Causal(CausalExtensions {
                        log_id,
                        timestamp,
                        previous,
                    })
                } else {
                    return Err(SerdeError::custom("unsupported extensions variant"));
                };

                Ok(Extensions { version, variant })
            }
        }

        deserializer.deserialize_seq(ExtensionsVisitor)
    }
}

/// Append-only log identifier used by the Node API.
#[derive(Clone, Copy, Debug, Ord, PartialOrd, PartialEq, Eq, StdHash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LogId(Hash);

impl LogId {
    /// Derive log id from a topic.
    ///
    /// Since topics are randomly generated we get the guarantee that every log and thus operation
    /// will be uniquely identifiable.
    ///
    /// To keep topic itself private we derive it with a BLAKE3 digest.
    pub fn from_topic(topic: Topic) -> Self {
        LogId(Hash::digest(topic.as_bytes()))
    }

    pub fn as_bytes(&self) -> &[u8; HASH_LEN] {
        self.0.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use p2panda_core::cbor::{decode_cbor, encode_cbor};
    use p2panda_core::{PruneFlag, Timestamp, Topic};

    use super::{
        BasicExtensions, CausalExtensions, EXTENSIONS_VERSION, Extensions, ExtensionsVariantV1,
        LogId,
    };

    #[test]
    fn derive_from_topic() {
        let topic = Topic::random();
        let log_id = LogId::from_topic(topic);
        assert_ne!(topic.as_bytes(), log_id.as_bytes());
    }

    #[test]
    fn serde_roundtrips() {
        let topic = Topic::random();

        {
            let basic = Extensions {
                version: EXTENSIONS_VERSION,
                variant: ExtensionsVariantV1::Basic(BasicExtensions {
                    log_id: LogId::from_topic(topic),
                    prune_flag: PruneFlag::default(),
                    timestamp: Timestamp::now(),
                }),
            };

            let bytes = encode_cbor(&basic).unwrap();
            let result: Extensions = decode_cbor(&bytes[..]).unwrap();
            assert_eq!(result, basic);
        }

        {
            let causal = Extensions {
                version: EXTENSIONS_VERSION,
                variant: ExtensionsVariantV1::Causal(CausalExtensions {
                    log_id: LogId::from_topic(topic),
                    timestamp: Timestamp::zero(),
                    previous: HashSet::from([]),
                }),
            };

            let bytes = encode_cbor(&causal).unwrap();
            let result: Extensions = decode_cbor(&bytes[..]).unwrap();
            assert_eq!(result, causal);
        }
    }

    #[test]
    fn deserialize_unknown_extension_fields() {
        let future_extension = (
            // We still use the old version as this is _not_ necessarily a breaking change.
            EXTENSIONS_VERSION,
            // .. same for the variant code.
            0x00,
            LogId::from_topic(Topic::random()),
            Timestamp::now(),
            PruneFlag::new(false),
            // We introduce the new field _at the end_ of the sequence.
            "our new field".to_string(),
        );
        let future_bytes = encode_cbor(&future_extension).unwrap();

        // .. and should make sure that it will not break previous versions.
        let result = decode_cbor::<Extensions, _>(&future_bytes[..]);
        assert!(
            result.is_ok(),
            "should be able to decode future, non-breaking extensions"
        );
    }
}
