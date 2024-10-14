// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::{validate_backlink, Header, OperationError};

pub fn validate_prunable_backlink<E>(
    past_header: Option<&Header<E>>,
    header: &Header<E>,
    prune_flag: bool,
) -> Result<(), OperationError>
where
    E: Clone + Serialize + for<'a> Deserialize<'a>,
{
    assert!(
        !(past_header.is_some() && header.seq_num == 0),
        "operation can't have backlink at seq_num = 0"
    );

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
    use crate::extensions::DefaultExtensions;
    use crate::{Hash, Header, PrivateKey};

    use super::validate_prunable_backlink;

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
}
