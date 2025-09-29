// SPDX-License-Identifier: MIT OR Apache-2.0

pub trait Ordering<ID> {
    fn dependencies(&self) -> &[ID];
}
