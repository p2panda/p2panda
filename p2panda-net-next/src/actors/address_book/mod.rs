// SPDX-License-Identifier: MIT OR Apache-2.0

#[allow(clippy::module_inception)]
mod address_book;
mod subscribers;

pub use address_book::{
    ADDRESS_BOOK, AddressBook, ImmediateResult, NodeEvent, ToAddressBook, TopicEvent,
};
