// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::hash::Hash as StdHash;

use p2panda_core::hash::{HASH_LEN, Hash};
use p2panda_core::{PruneFlag, Topic};
use serde::de::{Error as SerdeError, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};

/// Header type with our system-level extensions.
pub type Header = p2panda_core::Header<Extensions>;

/// Operation type with our system-level extensions.
pub type Operation = p2panda_core::Operation<Extensions>;

/// Versioning for internal extensions format.
pub(crate) const EXTENSIONS_VERSION: u64 = 1;

/// Header extensions used in the event processor pipeline to coordinate system-level concerns, for
/// example pruning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extensions {
    version: u64,
    variant: ExtensionsVariantV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionsVariantV1 {
    Basic(BasicExtensions),
    Causal(CausalExtensions),
}

impl ExtensionsVariantV1 {
    /// Unique code to identify each extension variant.
    pub fn code(&self) -> u8 {
        match self {
            ExtensionsVariantV1::Basic(_) => BasicExtensions::CODE,
            ExtensionsVariantV1::Causal(_) => CausalExtensions::CODE,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasicExtensions {
    #[serde(rename = "l")]
    pub log_id: LogId,

    #[serde(
        rename = "p",
        skip_serializing_if = "PruneFlag::is_not_set",
        default = "PruneFlag::default"
    )]
    pub prune_flag: PruneFlag,
}

impl BasicExtensions {
    const CODE: u8 = 0x00;
}

#[allow(unused)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CausalExtensions {
    #[serde(rename = "l")]
    pub log_id: LogId,

    #[serde(rename = "t")]
    pub previous: HashSet<Hash>,
}

impl CausalExtensions {
    const CODE: u8 = 0x01;
}

impl Extensions {
    pub fn from_topic(topic: Topic) -> Self {
        Self {
            version: EXTENSIONS_VERSION,
            variant: ExtensionsVariantV1::Basic(BasicExtensions {
                log_id: LogId::from_topic(topic),
                prune_flag: PruneFlag::default(),
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

    pub fn version(&self) -> u64 {
        self.version
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
            _ => unreachable!(),
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
        //    version[u64],
        //    extensions_variant_code[u8],
        //    {
        //      .. extensions w. fields ..
        //    }
        // )
        // ```
        let mut seq = serializer.serialize_seq(Some(3))?;

        seq.serialize_element(&self.version)?;
        seq.serialize_element(&self.variant.code())?;

        match &self.variant {
            ExtensionsVariantV1::Basic(extensions) => {
                seq.serialize_element(extensions)?;
            }
            ExtensionsVariantV1::Causal(extensions) => {
                seq.serialize_element(extensions)?;
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
                let version: u64 = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("version missing"))?;

                if version != EXTENSIONS_VERSION {
                    return Err(SerdeError::custom("unsupported extensions version"));
                }

                let variant_code: u8 = seq
                    .next_element()?
                    .ok_or(SerdeError::custom("variant code missing"))?;

                let variant = if variant_code == BasicExtensions::CODE {
                    let extensions: BasicExtensions = seq
                        .next_element()?
                        .ok_or(SerdeError::custom("extensions missing"))?;
                    ExtensionsVariantV1::Basic(extensions)
                } else if variant_code == CausalExtensions::CODE {
                    let extensions: CausalExtensions = seq
                        .next_element()?
                        .ok_or(SerdeError::custom("extensions missing"))?;
                    ExtensionsVariantV1::Causal(extensions)
                } else {
                    return Err(SerdeError::custom("unsupported extensions variant"));
                };

                if let Some(remaining) = seq.size_hint()
                    && remaining > 0
                {
                    return Err(SerdeError::custom(
                        "exceeded expected elements in extensions format",
                    ));
                }

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
    use p2panda_core::cbor::{decode_cbor, encode_cbor};
    use p2panda_core::{PruneFlag, Topic};
    use serde::{Deserialize, Serialize};

    use super::{EXTENSIONS_VERSION, Extensions, LogId};

    #[test]
    fn derive_from_topic() {
        let topic = Topic::random();
        let log_id = LogId::from_topic(topic);
        assert_ne!(topic.as_bytes(), log_id.as_bytes());
    }

    #[test]
    fn deserialize_unknown_extension_fields() {
        // We're introducing a new field into an "old" extensions variant in the future ..
        #[derive(Serialize, Deserialize)]
        struct FutureBasicExtensions {
            #[serde(rename = "l")]
            log_id: LogId,

            #[serde(rename = "f")]
            a_new_future_field: bool,

            #[serde(
                rename = "p",
                skip_serializing_if = "PruneFlag::is_not_set",
                default = "PruneFlag::default"
            )]
            prune_flag: PruneFlag,
        }

        // We still use the old version as this is _not_ necessarily a breaking change.
        let future_extension = (
            EXTENSIONS_VERSION,
            0x00,
            FutureBasicExtensions {
                log_id: LogId::from_topic(Topic::new()),
                a_new_future_field: false,
                prune_flag: false.into(),
            },
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
