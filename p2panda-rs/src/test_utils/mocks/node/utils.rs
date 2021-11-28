// SPDX-License-Identifier: AGPL-3.0-or-later

//! utilities and hard coded system schema values

/// hard coded meta schema system schema hash
pub const META_SCHEMA_HASH: &str = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce16079";

/// hard coded key package system schema hash
pub const KEY_PACKAGE_SCHEMA_HASH: &str = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6cb59430";

/// hard coded group system schema hash
pub const GROUP_SCHEMA_HASH: &str = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";

/// hard coded permission system schema hash
pub const PERMISSIONS_SCHEMA_HASH: &str = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce322e8";

/// A custom `Result` type to be able to dynamically propagate `Error` types.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
