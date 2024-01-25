// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod action;
pub mod decode;
pub mod encode;
pub mod encoded_header;
pub mod error;
#[allow(clippy::module_inception)]
mod header;
mod seq_num;
pub mod validate;

pub use action::HeaderAction;
pub use encoded_header::EncodedHeader;
pub use header::{DocumentLinks, Header, HeaderBuilder, HeaderExtension};
pub use seq_num::SeqNum;
