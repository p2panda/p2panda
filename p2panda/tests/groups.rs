// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda::{NetworkId, Node};

async fn spawn_node(network_id: NetworkId) -> Node {
    p2panda::builder()
        .network_id(network_id)
        .spawn()
        .await
        .unwrap()
}

mod groups_processor {
    use p2panda::{AccessLevel, Hash, Topic};
    use p2panda_core::test_utils::setup_logging;
    use p2panda_spaces::AuthGroupState;
    use p2panda_store::{groups::GroupsStore, tx_unwrap};

    use crate::spawn_node;

    #[tokio::test]
    async fn namespaced_states() {
        setup_logging();

        let network_id = Topic::random().into();
        let panda = spawn_node(network_id).await;

        // The "create" group message is consumed by the groups processor.
        let group = panda
            .create_group(&[(panda.id(), AccessLevel::Manage)])
            .await
            .unwrap();
        let store = panda.store();

        // We expect that state is namespaced by the group id.
        let group_retrieved: Option<AuthGroupState<()>> = tx_unwrap!(
            store,
            store
                .get_groups_state_tx(Hash::from_bytes(*group.id().as_bytes()))
                .await
        )
        .unwrap();

        assert!(group_retrieved.is_some());
        let group_retrieved = group_retrieved.unwrap();

        assert!(group_retrieved.has_group(group.id()));
    }
}
