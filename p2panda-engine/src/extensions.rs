// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ops::Deref;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PruneFlag(bool);

impl PruneFlag {
    pub fn new(flag: bool) -> Self {
        Self(flag)
    }

    pub fn is_set(&self) -> bool {
        self.0
    }

    pub fn is_not_set(&self) -> bool {
        !self.0
    }
}

impl From<bool> for PruneFlag {
    fn from(value: bool) -> Self {
        Self(value)
    }
}

impl Deref for PruneFlag {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::cbor::{decode_cbor, encode_cbor};
    use serde::{Deserialize, Serialize};

    use super::PruneFlag;

    #[test]
    fn prune_flag_encoding_is_short() {
        let prune_flag = PruneFlag::default();
        let bytes = encode_cbor(&prune_flag).unwrap();
        assert_eq!(bytes.len(), 1);
    }

    #[test]
    fn prune_flag_deref() {
        let prune_flag = PruneFlag::default();
        if *prune_flag {
            panic!("should be false!");
        }
    }

    #[test]
    fn prune_flag_can_be_optional() {
        #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
        struct Extensions {
            #[serde(
                rename = "p",
                skip_serializing_if = "PruneFlag::is_not_set",
                default = "PruneFlag::default"
            )]
            pub prune_flag: PruneFlag,
        }

        let extensions = Extensions::default();
        let bytes = encode_cbor(&extensions).unwrap();
        // A false "prune flag" will not be serialized at all
        assert_eq!(bytes.len(), 1);
        let decoded: Extensions = decode_cbor(&bytes).unwrap();
        assert_eq!(extensions, decoded);

        let extensions = Extensions {
            prune_flag: true.into(),
        };
        let bytes = encode_cbor(&extensions).unwrap();
        assert_eq!(bytes.len(), 4);
        let decoded: Extensions = decode_cbor(&bytes).unwrap();
        assert_eq!(extensions, decoded);
    }
}
