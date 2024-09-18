// SPDX-License-Identifier: AGPL-3.0-or-later

mod macros;
mod operation;
mod stream;
#[cfg(test)]
mod test_utils;

pub use stream::decode::{Decode, DecodeExt};
