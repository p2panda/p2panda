// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::AnyOperation;

#[allow(unused)]
pub enum EventDeliveryOperation {
    Encrypted(Vec<u8>),
    Plaintext(Box<AnyOperation>),
}

#[allow(unused)]
pub enum SubscriptionItem<MD, S> {
    Operation {
        operation: Box<EventDeliveryOperation>,
        meta_data: MD,
    },
    StatusUpdate(S),
}
