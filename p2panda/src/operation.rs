// SPDX-License-Identifier: MIT OR Apache-2.0

// ## Node Extensions Format
//
// The Node Extensions Format (NEF) defines a specialised extensions data-type used in p2panda's
// header `extensions` field. It contains information required by p2panda's high-level Node API to
// coordinate or trigger system-level event processing, for example log assignment, pruning or
// timestamps.
//
// This format is designed to make introducing new system-level processors or changes with forward-
// and backwards compatibility in mind. It aims at being efficiently encodable in CBOR.
//
// ### Definitions
//
// **Inputs:**
//
// Data-types (usually immutable) sent between systems.
//
// **Systems:**
//
// Software which encodes and decodes inputs and offers features on top of it.
//
// **Backwards Compatibility:**
//
// Backwards compatibility allows interoperability with an older legacy system, or with input
// designed for such a system. See: <https://en.wikipedia.org/wiki/Backward_compatibility>.
//
// - Old inputs remain valid when used with new systems.
// - New systems can process old inputs without breaking.
//
// **Forwards-Compatibility:**
//
// Forwards compatibility is about a system accepting input intended for a later version of itself.
// The focus is on gracefully handling newer input, often by ignoring parts it doesn't understand.
// See: <https://en.wikipedia.org/wiki/Forward_compatibility>.
//
// - Old systems can accept and gracefully process input intended for newer versions.
// - Old systems can ignore new/unknown parts without breaking.
//
// ### Specification
//
// #### Encoding
//
// NEF MUST be encoded as CBOR while an deterministic encoding SHOULD be preferred as defined in the
// CBOR specification:
// <https://www.rfc-editor.org/rfc/rfc8949.html#name-deterministically-encoded-c>. All following
// encoding definitions can be considered as CBOR. Refer to CBOR specification for byte-level
// definitions.
//
// #### Header
//
// Every NEF is represented as a tuple and begins with a `u16` **version** field:
//
// ```plain
// (
//    extensions_version[u16],
// )
// ```
//
// The latest (and only) supported NEF version is `1`. Version 1 requires another field indicating
// the type of **extensions variant** in form of a `u16` code:
//
// ```plain
// (
//    1,
//    extensions_variant_code[u16],
// )
// ```
//
// Decoders MUST silently ignore unknown / unsupported versions when reading the version or
// variant_code fields. Systems MAY support old versions and variants when possible.
//
// #### Extensions Variant
//
// NEF is a meta-format allowing to express different extension variants. New variants can easily be
// introduced through the extensions variant code.
//
// It is not recommended to represent extensions variants as a nested tuple or map but SHOULD rather
// be "flattened" inside the same header sequence to avoid redundant bytes. Like this we can say
// that extensions variants usually consist of **fields**.
//
// ```plain
// # Not recommended extension variants with redundant bytes are for example:
// (1, 0, { ... })
// (1, 0, (...))
//
// # More efficient, recommended variant is, where `...` represents any fields
// # required by the variant:
// (1, 0, ...)
// ```
//
// Optional fields in sequences MUST always be present and instead indicated by a "false" (`0xF4` in
// CBOR), "null" (`0xF6` in CBOR) or "zero" byte (`0x00` in CBOR), depending on the field's
// requirements. Omitting the optional field would break the indexes of other fields and cause
// undefined behaviour.
//
// ```plain
// # Correct: Encoding the 4th field as "unset" if it is optional:
// (1, 0, "hello", null, 122)
// (1, 0, "hello", 0, 122)
// (1, 0, "hello", false, 122)
//
// # Invalid: Omit the "unused" field and break indexes:
// (1, 0, "hello", 122)
// ```
//
// ### Implementation
//
// #### Allow excess fields when decoding
//
// Decoders MUST _never_ fail on excessive sequence fields in reasonable range as this would break
// any backwards compatibility, this roughly follows the [robustness
// principle](https://en.wikipedia.org/wiki/Robustness_principle). Decoders MAY fail when an
// extraordinarily large number of fields should be allocated (like more than 100 etc.).
//
// ```plain
// # Imagine an extensions variant introducing a new field `age`:
// (1, 0, username, age)
//
// # An older system decodes the variant as such:
// (1, 0, username)
//
// # The older system would break since it detected an unknown, "superfluous" field. We can
// # avoid this by ending decoding whenever we are happy with the fields we need and not checking
// # further.
// ```
//
// #### Design fallbacks when introducing new fields
//
// To assure that removing extension fields doesn't break anything for older versions (backwards
// compatibility) it is recommended to implement code to gracefully fall-back when a field is zero
// or null.
//
// If a graceful fallback can't be guaranteed it is recommended to document this in the variant's
// fields specification.
//
// ```rust
// // Deserializes to None if string is empty or null.
// let username: Option<String> = None;
//
// // Fall back to public key if username is not set.
// let username = username.unwrap_or(verifying_key);
// ```
//
// #### Design less strict decoders when introducing new fields
//
// Consider the possibility that your introduced field will change in the future and requires
// additional data to support more options.
//
// If your decoder is satisfied with the given data you MAY want to stop here and not require
// additional strict validation checks to match the _exact_ format.
//
// ### Known extensions variants table (draft)
//
// #### `0x00_00 (0)`: "Basic Extensions"
//
// TODO: Mention deterministic topic -> log id digest
// TODO: Mention millisecond-precision timestamp since UNIX epoch
//
// ```plain
// (
//     // Extension header
//     version[u16],
//     0x00_00[u16],
//
//     // Extension variant
//     log_id[32],              // min. 32 bytes need to be given
//     timestamp[u64],          // 0 is valid value
//     prune_flag[bool],        // false is no-op
// )
// ```
//
// #### `0x00_01 (1)`: "Space Extensions"
//
// TODO: Proper specification of all fields.
//
// ```plain
// (
//     // Extension header
//     version[u16],
//     0x00_01[u16],
//
//     // Extension variant
//     log_id[32],
//     timestamp[u64],
// )
// ```
//
// ### Introducing Changes
//
// #### Header
//
// The a) header formatted as an array b) CBOR encoding and c) first version field in the array MUST
// never be changed for this specification. Any changes here would require a new specification.
//
// Any other changes to the header format MUST be introduced by incrementing the Node Extensions
// Version. For example such as: `(2, some_new_field, extensions_variant_code, ...)`.
//
// Introducing a new header version doesn't break old systems as their decoders silently ignore
// unknown ("new") versions. Old systems will not be able to process any new input versions.
//
// Implementers of new systems MAY allow backwards compatibility of old header versions when
// possible.
//
// ```plain
// # Old input version:
// (1, ...)
//
// # New input version:
// (2, ...)
//  = <- new header version
//
// # From perspective of old system:
// (2, ..)
//  = <- ignore unknown version, it's a no-op
//
// # From perspective of new system:
// (1, ..)
//  = <- older variants are still supported or silently ignored
// ```
//
// #### Introduce new extensions variant
//
// Make sure to determine a new extensions variant code which has not been used yet (see table
// below).
//
// Introducing a new extensions variant doesn't break old systems as their decoders ignore unknown
// ("new") variants. Old systems will not be able to process any of these new inputs.
//
// Implementers of new code MAY allow backwards compatibility of old extensions versions when
// possible.
//
// ```plain
// # Old input version:
// (1, 0, timestamp)
//
// # New input version:
// (1, 1, username)
//     = <- new extension variant code
//
// # From perspective of old system:
// (1, 1, ..)
//     = <- ignore unknown variant code, it's a no-op
//
// # From perspective of new system:
// (1, 0, timestamp)
//     = <- other variants are still supported or silently ignored
// ```
//
// #### Remove field from existing variant
//
// It is not possible to remove existing fields from an extensions variant. Consider introducing a
// new variant if this is not an option.
//
// The following should be done when planning to remove a field:
//
// If the field is "nullable" (boolean = false, Option = None, integers = 0, etc.) then new versions
// can use this to express a "removed" field (it will always be unused).
//
// Backwards compatibility depends on if a "zero" or "null" value breaks any logic or can be handled
// with a graceful fallback in old systems. Refer to variant's specification for details.
//
// Non-zero values from the "removed" field of older extensions SHOULD be "nullified" / ignored for
// Forward-Compatibility.
//
// ```plain
// # Old input version:
// (1, 0, username, age)
//
// # New input version:
// (1, 0, null, age)
//        ==== <- "removed" field
//
// # From perspective of old system:
// (1, 0, null, age)
//        ==== <- should have fall-back in place when value is not set
//
// # From perspective of new system:
// (1, 0, username, age)
//        ======== <- should be ignored / nullified
// ```
//
// #### Change field from existing variant
//
// This depends heavily on the nature of the field and it's encoding. Changes from u32 -> u64 etc.
// are trivial for example.
//
// For all types you should check if the CBOR encoding can be interpreted in a backwards- and
// forwards compatible way.
//
// ```plain
// # Old input version:
// (1, 0, log_id[32])
//
// # New input version:
// (1, 0, log_id[32] OR log_id[32] + suffix)
//                      =================== <- added new log_id variant with suffix
//
// # From perspective of old system:
// (1, 0, log_id[32, .. ignore excess bytes])
//
// # From perspective of new system:
// (1, 0, log_id[32])
// ```
//
// If this is not possible, prefer introducing a new field instead.
//
// #### Add field to existing variant
//
// New fields MUST be added _at the end_ of the sequence. If this is not possible, consider
// introducing a new extensions variant.
//
// Use `Option` and allow graceful fallbacks of an unset field for forward compatibility to make
// sure unset values from older inputs are still considered valid.
//
// This is forward-compatible with older systems as they will ignore any unknown ("new") excess
// fields.
//
// ```plain
// # Old input version:
// (1, 0, username)
//
// # New input version:
// (1, 0, username, age)
//                  === <- added field
//
// # From perspective of old system:
// (1, 0, username, [.. ignored])
//
// # From perspective of new system:
// (1, 0, username, null)
// ```
//
// #### Decision Matrix
//
// | Change Type        | Backwards Compatible   | Forwards Compatible
// | -------------------| ---------------------- | ------------------------
// | Add header version | Yes (need legacy code) | Yes (ignored)
// | Add variant        | Yes (need legacy code) | Yes (ignored)
// | Add field          | Maybe (if null-safe)   | Yes (ignored)
// | Remove field       | Yes (nullify non-zero) | Maybe (if null-safe)
// | Change field       | Maybe (CBOR-dependent) | Maybe (CBOR-dependent)
//
use std::hash::Hash as StdHash;

use p2panda_core::hash::{HASH_LEN, Hash};
use p2panda_core::{PruneFlag, Timestamp, Topic};
use serde::de::{Error as SerdeError, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};

use crate::spaces::types::SpacesArgs;

/// Extensions version type.
pub type Version = u16;

/// Extensions variant code type.
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
    pub version: Version,
    pub variant: ExtensionsVariantV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum ExtensionsVariantV1 {
    Basic(BasicExtensions),
    Space(SpaceExtensions),
}

impl ExtensionsVariantV1 {
    /// Unique code to identify each extension variant.
    pub fn code(&self) -> VariantCode {
        match self {
            ExtensionsVariantV1::Basic(_) => BasicExtensions::VARIANT_CODE,
            ExtensionsVariantV1::Space(_) => SpaceExtensions::VARIANT_CODE,
        }
    }
}

pub struct Builder {
    pub log_id: LogId,
    pub timestamp: Timestamp,
    pub prune_flag: PruneFlag,
}

impl Builder {
    pub fn new(log_id: LogId) -> Self {
        Self {
            log_id,
            timestamp: Timestamp::now(),
            prune_flag: PruneFlag::default(),
        }
    }

    pub fn prune_flag(mut self, prune_flag: impl Into<PruneFlag>) -> Self {
        self.prune_flag = prune_flag.into();
        self
    }

    pub fn timestamp(mut self, timestamp: impl Into<Timestamp>) -> Self {
        self.timestamp = timestamp.into();
        self
    }

    /// Returns "basic" extensions.
    pub fn build(self) -> Extensions {
        Extensions {
            version: EXTENSIONS_VERSION,
            variant: ExtensionsVariantV1::Basic(BasicExtensions {
                log_id: self.log_id,
                timestamp: self.timestamp,
                prune_flag: self.prune_flag,
            }),
        }
    }

    /// Returns "space" extensions.
    pub fn build_space(self, args: SpacesArgs) -> Extensions {
        Extensions {
            version: EXTENSIONS_VERSION,
            variant: ExtensionsVariantV1::Space(SpaceExtensions {
                log_id: self.log_id,
                timestamp: self.timestamp,
                args,
            }),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceExtensions {
    pub log_id: LogId,
    pub timestamp: Timestamp,

    // TODO: We likely want to split spaces args into more extensions types. At least one for
    // application messages & key bundles, potentially also auth / group messages.
    //
    // TODO: The serialization of spaces args is not-optimal, we probably want to bring our own
    // types in here and convert between them.
    //
    // TODO: Is causally ordering information encoded here or in another field?
    pub args: SpacesArgs,
}

impl SpaceExtensions {
    const VARIANT_CODE: VariantCode = 0x00_01;
    const FIELDS_COUNT: usize = 3;
}

impl Extensions {
    pub fn from_topic(topic: Topic) -> Extensions {
        Builder::new(LogId::from_topic(topic)).build()
    }

    pub fn builder(log_id: LogId) -> Builder {
        Builder::new(log_id)
    }

    pub fn version(&self) -> Version {
        self.version
    }

    #[allow(unused)]
    pub(crate) fn variant_code(&self) -> VariantCode {
        match &self.variant {
            ExtensionsVariantV1::Basic(_) => BasicExtensions::VARIANT_CODE,
            ExtensionsVariantV1::Space(_) => SpaceExtensions::VARIANT_CODE,
        }
    }

    pub(crate) fn fields_count(&self) -> usize {
        // (version, variant_code, ...)
        let header_field_count = 2;

        let variant_field_count = match &self.variant {
            ExtensionsVariantV1::Basic(_) => BasicExtensions::FIELDS_COUNT,
            ExtensionsVariantV1::Space(_) => SpaceExtensions::FIELDS_COUNT,
        };

        header_field_count + variant_field_count
    }

    pub fn log_id(&self) -> LogId {
        match &self.variant {
            ExtensionsVariantV1::Basic(extensions) => extensions.log_id,
            ExtensionsVariantV1::Space(extensions) => extensions.log_id,
        }
    }

    pub fn prune_flag(&self) -> PruneFlag {
        match &self.variant {
            ExtensionsVariantV1::Basic(extensions) => extensions.prune_flag,
            ExtensionsVariantV1::Space(_) => false.into(),
        }
    }

    pub fn timestamp(&self) -> Timestamp {
        match &self.variant {
            ExtensionsVariantV1::Basic(extensions) => extensions.timestamp,
            ExtensionsVariantV1::Space(extensions) => extensions.timestamp,
        }
    }

    pub fn spaces_args(&self) -> Option<SpacesArgs> {
        match &self.variant {
            ExtensionsVariantV1::Basic(_) => None,
            ExtensionsVariantV1::Space(extensions) => Some(extensions.args.clone()),
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
                seq.serialize_element(&extensions.log_id)?;
                seq.serialize_element(&extensions.timestamp)?;
                seq.serialize_element(&extensions.prune_flag)?;
            }
            ExtensionsVariantV1::Space(extensions) => {
                seq.serialize_element(&extensions.log_id)?;
                seq.serialize_element(&extensions.timestamp)?;
                seq.serialize_element(&extensions.args)?;
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
                } else if variant_code == SpaceExtensions::VARIANT_CODE {
                    let log_id: LogId = seq
                        .next_element()?
                        .ok_or(SerdeError::custom("log id missing"))?;

                    let timestamp: Timestamp = seq
                        .next_element()?
                        .ok_or(SerdeError::custom("timestamp missing"))?;

                    let args: SpacesArgs = seq
                        .next_element()?
                        .ok_or(SerdeError::custom("spaces arguments missing"))?;

                    ExtensionsVariantV1::Space(SpaceExtensions {
                        log_id,
                        timestamp,
                        args,
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
    pub fn digest(bytes: &[u8]) -> Self {
        Self(Hash::digest(bytes))
    }

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

impl From<Hash> for LogId {
    fn from(value: Hash) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::cbor::{decode_cbor, encode_cbor};
    use p2panda_core::{PruneFlag, Timestamp, Topic};

    use super::{BasicExtensions, EXTENSIONS_VERSION, Extensions, ExtensionsVariantV1, LogId};

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
    }

    #[test]
    fn backwards_compatible_extensions_fields() {
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

        // .. and should make sure that this new input will not break old systems.
        let result = decode_cbor::<Extensions, _>(&future_bytes[..]);
        assert!(
            result.is_ok(),
            "should be able to decode future, non-breaking extensions"
        );
    }
}
