// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::{validate_backlink, Header, OperationError};

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

/// Alternative backlink validation method for logs which allow pruning.
///
/// When a "prune flag" is set in an operation, an author signals to others that all operations can
/// be deleted (including payloads) in that log _before_ it.
///
/// ```text
/// Log of Author A with six Operations:
///
/// [ 0 ] <-- can be removed
/// [ 1 ] <-- can be removed
/// [ 2 ] <-- can be removed
/// [ 3 ] <-- prune flag = true
/// [ 4 ]
/// [ 5 ]
/// ...
/// ```
///
/// As soon as a prune flag was set for an operation we don't expect the header of a backlink,
/// otherwise we go on with validation as usual.
///
/// Use this method instead of [`validate_backlink`] if you want to support prunable logs.
pub fn validate_prunable_backlink<E>(
    past_header: Option<&Header<E>>,
    header: &Header<E>,
    prune_flag: bool,
) -> Result<(), OperationError>
where
    E: Clone + Serialize + for<'a> Deserialize<'a>,
{
    // If no pruning flag is set, we expect the log to have integrity with the previously given
    // operation
    if !prune_flag && header.seq_num > 0 {
        match past_header {
            Some(past_header) => validate_backlink(past_header, header),
            None => Err(OperationError::BacklinkMissing),
        }
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use crate::cbor::{decode_cbor, encode_cbor};
    use crate::extensions::DefaultExtensions;
    use crate::{Hash, Header, PrivateKey};

    use super::{validate_prunable_backlink, PruneFlag};

    #[test]
    fn validate_pruned_log() {
        let private_key = PrivateKey::new();
        let mut header = Header::<DefaultExtensions> {
            public_key: private_key.public_key(),
            seq_num: 7,
            backlink: Some(Hash::new(&[1, 2, 3])),
            ..Default::default()
        };
        header.sign(&private_key);

        // When no pruning flag was set we expect a backlink for this operation at seq_num = 7,
        // otherwise not
        assert!(validate_prunable_backlink(None, &header, false).is_err());
        assert!(validate_prunable_backlink(None, &header, true).is_ok());
    }

    #[test]
    fn seq_num_zero() {
        let private_key = PrivateKey::new();
        let mut header = Header::<DefaultExtensions> {
            public_key: private_key.public_key(),
            ..Default::default()
        };
        header.sign(&private_key);

        // Everything is fine at the beginning of the log
        assert!(validate_prunable_backlink(None, &header, false).is_ok());
        assert!(validate_prunable_backlink(None, &header, true).is_ok());
    }

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
