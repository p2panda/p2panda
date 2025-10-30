// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`Extension`](crate::Extension) representing points in a log where all preceding operations can
//! be deleted.
//!
//! `PruneFlag` is a built-in p2panda header extension which is required when using
//! `p2panda-stream`. It allows users to define points in a log where all previous operations can
//! be deleted. When operations arrive on a peer using `p2panda-stream` for ingesting messages,
//! garbage collection will automatically occur and eventually data will be removed network-wide.
//!
//! The process by which eligible prune points are established is an application layer concern. It
//! could be that messages of a certain age are no longer retained, or that changes to a CRDT-like
//! data type have been flagged for garbage collection.
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::{Extensions, Header, OperationError, validate_backlink};

/// Flag indicating that all preceding operations in a log can be deleted.
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
    E: Extensions,
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
    use crate::{Hash, Header, PrivateKey};

    use super::{PruneFlag, validate_prunable_backlink};

    #[test]
    fn validate_pruned_log() {
        let private_key = PrivateKey::new();
        let mut header = Header::<()> {
            public_key: private_key.public_key(),
            seq_num: 7,
            backlink: Some(Hash::new([1, 2, 3])),
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
        let mut header = Header::<()> {
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
        let decoded: Extensions = decode_cbor(&bytes[..]).unwrap();
        assert_eq!(extensions, decoded);

        let extensions = Extensions {
            prune_flag: true.into(),
        };
        let bytes = encode_cbor(&extensions).unwrap();
        assert_eq!(bytes.len(), 4);
        let decoded: Extensions = decode_cbor(&bytes[..]).unwrap();
        assert_eq!(extensions, decoded);
    }
}
