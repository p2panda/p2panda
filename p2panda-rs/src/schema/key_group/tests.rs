// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::rstest;

use crate::document::{DocumentId, DocumentViewId};
use crate::identity::{Author, KeyPair};

use crate::schema::key_group::Owner;
use crate::test_utils::fixtures::random_key_pair;
use crate::test_utils::mocks::{send_to_node, Client, Node};

use super::KeyGroup;

#[rstest]
fn key_group_management(
    #[from(random_key_pair)] frog_key_pair: KeyPair,
    #[from(random_key_pair)] rabbit_key_pair: KeyPair,
) {
    let frog = Client::new("frog".to_string(), frog_key_pair);
    let frog_author = Author::new(&frog.public_key()).unwrap();

    let rabbit = Client::new("rabbit".to_string(), rabbit_key_pair);
    let rabbit_author = Author::new(&rabbit.public_key()).unwrap();

    let mut node = Node::new();

    // Frog creates the 'Strawberry Picking Gang' key group
    let (create_hash, _) = send_to_node(
        &mut node,
        &frog,
        &KeyGroup::create("Strawberry Picking Gang"),
    )
    .unwrap();

    let key_group_id: DocumentId = create_hash.into();

    let key_group =
        KeyGroup::new_from_documents(key_group_id.clone(), &node.get_documents(), &[]).unwrap();

    // ... and makes herself a member
    let (frog_request_doc_id, _) = send_to_node(
        &mut node,
        &frog,
        &key_group.request_membership(&frog_author.clone().into()),
    )
    .unwrap();

    let (frog_membership_doc_id, _) = send_to_node(
        &mut node,
        &frog,
        &KeyGroup::respond_to_request(&DocumentViewId::from(frog_request_doc_id), true),
    )
    .unwrap();

    let key_group =
        KeyGroup::new_from_documents(key_group_id.clone(), &node.get_documents(), &[]).unwrap();

    assert!(key_group.is_member(&frog_author));
    let expected_key_group_id = key_group_id.as_str().parse::<DocumentId>().unwrap();
    assert_eq!(key_group.id(), &expected_key_group_id);

    // Rabbit asks to become a member as well
    let (rabbit_request_doc_id, _) = send_to_node(
        &mut node,
        &rabbit,
        &key_group.request_membership(&rabbit_author.clone().into()),
    )
    .unwrap();
    node.get_document(&rabbit_request_doc_id);

    // But rabbit is not a member yet
    let key_group =
        KeyGroup::new_from_documents(key_group_id.clone(), &node.get_documents(), &[]).unwrap();

    assert!(!key_group.is_member(&rabbit_author));

    // Now frog let's rabbit in :)
    send_to_node(
        &mut node,
        &frog,
        &KeyGroup::respond_to_request(&DocumentViewId::from(rabbit_request_doc_id), true),
    )
    .unwrap();

    let key_group =
        KeyGroup::new_from_documents(key_group_id.clone(), &node.get_documents(), &[]).unwrap();

    assert!(key_group.is_member(&rabbit_author));

    // But rabbit would rather pick strawberries alone.
    send_to_node(
        &mut node,
        &rabbit,
        &KeyGroup::update_membership(&DocumentViewId::from(frog_membership_doc_id), false),
    )
    .unwrap();

    let key_group =
        KeyGroup::new_from_documents(key_group_id.clone(), &node.get_documents(), &[]).unwrap();

    assert!(!key_group.is_member(&frog_author));

    // So Frog makes a new key group for blueberry picking that has the whole strawberry
    // picking gang in it.

    let (blueberry_id, _) = send_to_node(
        &mut node,
        &frog,
        &KeyGroup::create("Blueberry Picking Gang"),
    )
    .unwrap();

    let blueberry_picking_gang =
        KeyGroup::new_from_documents(blueberry_id.clone().into(), &node.get_documents(), &[])
            .unwrap();

    let (frog_blueberry_request_doc_id, _) = send_to_node(
        &mut node,
        &frog,
        &blueberry_picking_gang.request_membership(&frog_author.clone().into()),
    )
    .unwrap();

    send_to_node(
        &mut node,
        &frog,
        &KeyGroup::respond_to_request(&DocumentViewId::from(frog_blueberry_request_doc_id), true),
    )
    .unwrap();

    // Rabbit concedes and asks for the whole strawberry picking gang to also become members
    let (spg_blueberry_request_doc_id, _) = send_to_node(
        &mut node,
        &rabbit,
        &blueberry_picking_gang.request_membership(&key_group.clone().into()),
    )
    .unwrap();

    send_to_node(
        &mut node,
        &frog,
        &KeyGroup::respond_to_request(&DocumentViewId::from(spg_blueberry_request_doc_id), true),
    )
    .unwrap();

    let blueberry_picking_gang = KeyGroup::new_from_documents(
        blueberry_id.into(),
        &node.get_documents(),
        &[key_group.clone()],
    )
    .unwrap();

    // Rabbit is a member by way of the Strawberry Picking Gang
    assert_eq!(
        blueberry_picking_gang.get(&rabbit_author).unwrap().member(),
        &Owner::KeyGroup(key_group.id().clone()),
        "{:?}",
        blueberry_picking_gang.get(&rabbit_author)
    );

    // Frog is not a member as part of the Strawberry Picking Gang because she added herself
    // directly to the group and her membership in the SPG is void.
    assert_eq!(
        blueberry_picking_gang.get(&frog_author).unwrap().member(),
        &Owner::Author(frog_author.clone()),
        "{:?}",
        blueberry_picking_gang.get(&frog_author)
    );
}
