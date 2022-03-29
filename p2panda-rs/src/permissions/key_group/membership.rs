// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::permissions::key_group::error::KeyGroupError;
use crate::permissions::key_group::{MembershipRequestView, MembershipResponseView, Owner};

/// Memership in a key group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Membership {
    /// True if this membership is accepted through a valid and positive response.
    accepted: bool,

    /// True if the request for this membership has received any response.
    has_response: bool,

    /// Reference to the public key or key group member.
    member: Owner,
}

impl Membership {
    /// Create a new membership instance.
    ///
    /// Use this constructor if you have validated the membership already.
    ///
    /// `member` is the identity whose membership is represented
    /// `response` encodes whether a valid response is known and whether that response was to
    ///   accept or reject the membership.
    pub fn new(member: &Owner, response: Option<bool>) -> Self {
        Self {
            accepted: response.is_some() && response.unwrap(),
            has_response: response.is_some(),
            member: member.clone(),
        }
    }

    /// Parse membership from a request and optional response.
    ///
    /// Request and response must match on the response's `request` field.
    pub fn from_confirmation(
        request: MembershipRequestView,
        response: Option<MembershipResponseView>,
    ) -> Result<Self, KeyGroupError> {
        match response {
            Some(response) => {
                if response.request() != request.view_id() {
                    return Err(KeyGroupError::InvalidMembership(
                        "response doesn't reference supplied request".to_string(),
                    ));
                }
                Ok(Membership {
                    accepted: *response.accepted(),
                    has_response: true,
                    member: request.member(),
                })
            }
            None => Ok(Membership {
                accepted: false,
                has_response: false,
                member: request.member(),
            }),
        }
    }

    /// Access the [`Owner`] whose membership this describes.
    pub fn member(&self) -> &Owner {
        &self.member
    }

    /// Returns true if this membership is accepted.
    ///
    /// Memberships that are not accepted have been revoked and should be considered void.
    pub fn accepted(&self) -> bool {
        self.accepted
    }

    /// Returns true if this membership has a valid response.
    pub fn has_response(&self) -> bool {
        self.has_response
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryInto;

    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::identity::{Author, KeyPair};
    use crate::permissions::key_group::{KeyGroup, Membership};
    use crate::test_utils::fixtures::random_key_pair;
    use crate::test_utils::mocks::{send_to_node, Client, Node};

    #[rstest]
    fn parse_membership(#[from(random_key_pair)] frog_key_pair: KeyPair) {
        // Test setup
        let frog = Client::new("frog".to_string(), frog_key_pair);
        let frog_author = Author::new(&frog.public_key()).unwrap();

        let mut node = Node::new();

        // Frog creates the 'Strawberry Picking Gang' key group
        let (create_hash, _) = send_to_node(
            &mut node,
            &frog,
            &KeyGroup::create("Strawberry Picking Gang"),
        )
        .unwrap();

        let key_group =
            KeyGroup::new_from_documents(DocumentId::from(create_hash), &node.get_documents(), &[])
                .unwrap();

        // ... and requests membership
        let (frog_request_doc_id, _) =
            send_to_node(&mut node, &frog, &key_group.request_membership(None)).unwrap();

        // She should be an unconfirmed memberb
        let membership = Membership::from_confirmation(
            node.get_document(&frog_request_doc_id).try_into().unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(
            membership,
            Membership::new(&frog_author.clone().into(), None)
        );
        assert!(!membership.accepted());

        // She responds to the request
        let (frog_membership_doc_id, _) = send_to_node(
            &mut node,
            &frog,
            &KeyGroup::respond_to_request(&DocumentViewId::from(frog_request_doc_id.clone()), true),
        )
        .unwrap();

        // She should be a confirmed member
        let membership = Membership::from_confirmation(
            node.get_document(&frog_request_doc_id).try_into().unwrap(),
            Some(
                node.get_document(&frog_membership_doc_id)
                    .try_into()
                    .unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(
            membership,
            Membership::new(&frog_author.clone().into(), Some(true))
        );
        assert!(membership.accepted());

        // She revokes her membership
        send_to_node(
            &mut node,
            &frog,
            &KeyGroup::update_membership(
                &DocumentViewId::from(frog_membership_doc_id.clone()),
                false,
            ),
        )
        .unwrap();

        // She should not be a member anymore
        let membership = Membership::from_confirmation(
            node.get_document(&frog_request_doc_id).try_into().unwrap(),
            Some(
                node.get_document(&frog_membership_doc_id)
                    .try_into()
                    .unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(
            membership,
            Membership::new(&frog_author.into(), Some(false))
        );
        assert!(!membership.accepted());
    }
}
