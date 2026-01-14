// SPDX-License-Identifier: MIT OR Apache-2.0

#[derive(Clone, Default)]
pub enum RestartStrategy {
    #[default]
    OneForOne,
    OneForAll,
}
