// SPDX-License-Identifier: AGPL-3.0-or-later

#[allow(clippy::module_inception)]
mod body;
pub mod decode;
pub mod encode;
mod encoded_body;
pub mod error;
mod fields;
mod value;

pub use body::Body;
pub use encoded_body::EncodedBody;
pub use fields::PlainFields;
pub use value::PlainValue;
