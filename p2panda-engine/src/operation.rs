// SPDX-License-Identifier: AGPL-3.0-or-later

/// Encoded bytes of an operation header and optional body.
pub type RawOperation = (Vec<u8>, Option<Vec<u8>>);
