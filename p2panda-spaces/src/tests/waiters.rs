// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use crate::test_utils::TestPeer;

#[tokio::test]
async fn own_key_bundle_notifies_member_waiters() {
    let alice = TestPeer::new(0).await;
    let alice_id = alice.manager.id();

    let waiter = alice
        .manager
        .waiters()
        .members
        .wait_for(move |member| member.id() == alice_id);

    // First call generates and registers our own key bundle.
    alice.manager.me().await.unwrap();

    tokio::time::timeout(Duration::from_secs(1), waiter)
        .await
        .expect("waiter resolved");
}
