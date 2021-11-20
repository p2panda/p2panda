// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for `SecretGroup`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum SecretGroupError {}
