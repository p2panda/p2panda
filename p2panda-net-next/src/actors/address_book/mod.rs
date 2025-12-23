// SPDX-License-Identifier: MIT OR Apache-2.0

#[allow(clippy::module_inception)]
mod address_book;

use std::collections::HashSet;

use ractor::{ActorRef, call, registry};
use thiserror::Error;

use crate::actors::{ActorNamespace, with_namespace};
use crate::addrs::{NodeInfo, TransportInfo};
use crate::watchers::{UpdatesOnly, WatcherReceiver};
use crate::{NodeId, TopicId};

pub use address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};

pub async fn address_book_ref(actor_namespace: ActorNamespace) -> Option<ActorRef<ToAddressBook>> {
    registry::where_is(with_namespace(ADDRESS_BOOK, &actor_namespace))
        .map(ActorRef::<ToAddressBook>::from)
}

pub async fn node_info(
    actor_namespace: ActorNamespace,
    node_id: NodeId,
) -> Result<Option<NodeInfo>, AddressBookUtilsError> {
    let Some(address_book_ref) = address_book_ref(actor_namespace).await else {
        return Err(AddressBookUtilsError::ActorNotAvailable);
    };

    let result = call!(address_book_ref, ToAddressBook::NodeInfo, node_id)
        .map_err(|_| AddressBookUtilsError::ActorFailed)?;

    Ok(result)
}

pub async fn update_address_book(
    actor_namespace: ActorNamespace,
    node_id: NodeId,
    transport_info: TransportInfo,
) -> Result<(), AddressBookUtilsError> {
    let Some(address_book_ref) = address_book_ref(actor_namespace).await else {
        return Err(AddressBookUtilsError::ActorNotAvailable);
    };

    let _ = call!(
        address_book_ref,
        ToAddressBook::InsertTransportInfo,
        node_id,
        transport_info
    )
    .map_err(|_| AddressBookUtilsError::ActorFailed)?;

    Ok(())
}

pub async fn watch_node_info(
    actor_namespace: ActorNamespace,
    node_id: NodeId,
    updates_only: UpdatesOnly,
) -> Result<WatcherReceiver<Option<NodeInfo>>, AddressBookUtilsError> {
    let Some(address_book_ref) = address_book_ref(actor_namespace).await else {
        return Err(AddressBookUtilsError::ActorNotAvailable);
    };

    let rx = call!(
        address_book_ref,
        ToAddressBook::WatchNodeInfo,
        node_id,
        updates_only
    )
    .map_err(|_| AddressBookUtilsError::ActorFailed)?;

    Ok(rx)
}

pub async fn watch_topic(
    actor_namespace: ActorNamespace,
    topic: TopicId,
    updates_only: UpdatesOnly,
) -> Result<WatcherReceiver<HashSet<NodeId>>, AddressBookUtilsError> {
    let Some(address_book_ref) = address_book_ref(actor_namespace).await else {
        return Err(AddressBookUtilsError::ActorNotAvailable);
    };

    let rx = call!(
        address_book_ref,
        ToAddressBook::WatchTopic,
        topic,
        updates_only
    )
    .map_err(|_| AddressBookUtilsError::ActorFailed)?;

    Ok(rx)
}

pub async fn watch_node_topics(
    actor_namespace: ActorNamespace,
    node_id: NodeId,
    updates_only: UpdatesOnly,
) -> Result<WatcherReceiver<HashSet<TopicId>>, AddressBookUtilsError> {
    let Some(address_book_ref) = address_book_ref(actor_namespace).await else {
        return Err(AddressBookUtilsError::ActorNotAvailable);
    };

    let rx = call!(
        address_book_ref,
        ToAddressBook::WatchNodeTopics,
        node_id,
        updates_only
    )
    .map_err(|_| AddressBookUtilsError::ActorFailed)?;

    Ok(rx)
}

#[derive(Debug, Error)]
pub enum AddressBookUtilsError {
    #[error("address book actor is not available")]
    ActorNotAvailable,

    #[error("address book actor failed")]
    ActorFailed,
}
