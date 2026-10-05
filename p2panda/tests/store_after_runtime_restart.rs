// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_store::Transaction;

#[test]
fn app_can_write_after_runtime_restart_interrupted_a_write() {
    let runtime = || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    };

    let first = runtime();
    let store = first.block_on(async {
        let node = p2panda::builder().spawn().await.unwrap();
        let store = node.store();

        // The application starts a write of its own and is interrupted before committing it.
        let interrupted_write = store.begin().await.unwrap();
        drop(interrupted_write);

        store
    });
    // The runtime stops here, before the interrupted write was rolled back.
    drop(first);

    let second = runtime();
    second.block_on(async {
        let write = store
            .begin()
            .await
            .expect("the application can write to the database after the restart");
        store.commit(write).await.unwrap();
    });
}
