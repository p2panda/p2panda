// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::crypto::x25519::SecretKey;

pub trait IdentityManager<Y> {
    fn identity_secret(y: &Y) -> &SecretKey;
}
