// SPDX-License-Identifier: AGPL-3.0-or-later

// p2panda schema hashes
pub const META_SCHEMA_HASH: &str = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce16079";
pub const KEY_PACKAGE_SCHEMA_HASH: &str = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6cb59430";
pub const GROUP_SCHEMA_HASH: &str = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";
pub const PERMISSIONS_SCHEMA_HASH: &str = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce322e8";

// A custom `Result` type to be able to dynamically propagate `Error` types.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

// p2panda system schema

pub const META_SCHEMA: &str = r#"
meta-schema = { (
    cddl-str: { type: "str", value: tstr },
) }
"#;

// p2panda user schema

pub const CHAT_SCHEMA: &str = r#"
chat = { (
    message: { type: "str", value: tstr },
) }
"#;
