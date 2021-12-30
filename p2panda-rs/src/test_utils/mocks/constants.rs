// SPDX-License-Identifier: AGPL-3.0-or-later

//! Constant system schema values.

/// "Meta Schema" system schema hash.
pub const META_SCHEMA_HASH: &str =
    "0020add61d217fbd5b908646176d779a5d09998d01394744dc244dfae629ba807425";

/// "Key Package" system schema hash.
pub const KEY_PACKAGE_SCHEMA_HASH: &str =
    "0020fec174e369e7966ed871b46089f482ae6fc8f8004891cd3b3ae731868e3336e2";

/// "Group" system schema hash.
pub const GROUP_SCHEMA_HASH: &str =
    "0020b059688d5b5a0612775d1e170cbf9644f1a6074e31302b2b542cbe9247426cc2";

/// "Permission" system schema hash.
pub const PERMISSIONS_SCHEMA_HASH: &str =
    "00203bb64522395b259d5d1b68ad638b77e7aade232468d2cb3c9928eb19f18d0bfb";

#[cfg(test)]
mod tests {
    use crate::hash::Hash;

    use super::*;

    #[test]
    fn schema_hashes() {
        let hash_value = Hash::new_from_bytes(vec![0, 0, 1]).unwrap();
        assert_eq!(hash_value.as_str(), META_SCHEMA_HASH);

        let hash_value = Hash::new_from_bytes(vec![0, 0, 2]).unwrap();
        assert_eq!(hash_value.as_str(), KEY_PACKAGE_SCHEMA_HASH);

        let hash_value = Hash::new_from_bytes(vec![0, 0, 3]).unwrap();
        assert_eq!(hash_value.as_str(), GROUP_SCHEMA_HASH);

        let hash_value = Hash::new_from_bytes(vec![0, 0, 4]).unwrap();
        assert_eq!(hash_value.as_str(), PERMISSIONS_SCHEMA_HASH);
    }
}
