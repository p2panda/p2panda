// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::str::FromStr;

use crate::{AddressBook, NodeId};

#[tokio::test]
async fn add_topic_bug() {
    // Related issue: https://github.com/p2panda/p2panda/issues/946
    let address_book = AddressBook::builder().spawn().await.unwrap();

    let node_id =
        NodeId::from_str("008136727520488c3755a66e968a1d2ded11eab83d8f5692011963aed788ae15")
            .unwrap();

    address_book.add_topic(node_id, [1; 32]).await.unwrap();
    address_book.add_topic(node_id, [2; 32]).await.unwrap();
    address_book.add_topic(node_id, [3; 32]).await.unwrap();

    let mut watcher = address_book
        .watch_node_topics(node_id, false)
        .await
        .unwrap();

    assert_eq!(
        watcher.recv().await.unwrap().value,
        HashSet::from_iter([[1; 32], [2; 32], [3; 32]])
    );
}
