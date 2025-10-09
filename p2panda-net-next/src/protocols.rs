// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use iroh::protocol::DynProtocolHandler as ProtocolHandler;

pub(crate) type ProtocolMap = BTreeMap<Vec<u8>, Box<dyn ProtocolHandler>>;
