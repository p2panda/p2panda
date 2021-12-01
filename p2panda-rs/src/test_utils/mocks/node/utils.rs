// SPDX-License-Identifier: AGPL-3.0-or-later

//! utilities and hard coded system schema values

/// hard coded meta schema system schema hash
pub const META_SCHEMA_HASH: &str =
    "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";

/// hard coded key package system schema hash
pub const KEY_PACKAGE_SCHEMA_HASH: &str =
    "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";

/// hard coded group system schema hash
pub const GROUP_SCHEMA_HASH: &str =
    "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";

/// hard coded permission system schema hash
pub const PERMISSIONS_SCHEMA_HASH: &str =
    "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";

/// A custom `Result` type to be able to dynamically propagate `Error` types.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
