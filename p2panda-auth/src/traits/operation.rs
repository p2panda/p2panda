// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{Author, OperationId};

use crate::group::{GroupAction, GroupMember};

/// The principal whose authority was used to validate an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(serde::Deserialize, serde::Serialize)
)]
pub enum AuthorizationActor<ID> {
    /// A group member whose access is checked by p2panda-auth at the operation's dependencies.
    Member(ID),
    /// The caller validated this operation through an external authorization model. The trusted
    /// path applies only to Add/Remove; Promote/Demote still require a member principal.
    Verified,
}

/// Interface to express required information from operations processed by any auth graph
/// implementation.
///
/// Applications implementing these traits should authenticate the original sender of each
/// operation.
pub trait Operation<ID, OP, C = ()>
where
    ID: Author,
    OP: OperationId,
{
    /// Id of this operation.
    fn id(&self) -> OP;

    /// ID of the author of this operation.
    fn author(&self) -> ID;

    /// Optional principal whose authority validates this operation.
    ///
    /// The operation remains authored by [`Self::author`] for attribution and resolver rules.
    /// `Member` is checked against group access at the operation's dependencies. `Verified`
    /// means the caller has already checked authority through another model, such as a delegated
    /// grant chain. Callers must never return `Verified` before that check succeeds. When `None`,
    /// the author is also the authorization principal.
    fn authorization_actor(&self) -> Option<AuthorizationActor<ID>> {
        None
    }

    /// Actor whose membership access is used when applying this operation.
    ///
    /// An actor removing themself is always treated as the author: leaving a group requires no
    /// delegated authorization and remains distinct from removing another member.
    fn permission_actor(&self) -> AuthorizationActor<ID> {
        let author = self.author();
        let is_leaving = matches!(
            self.action(),
            GroupAction::Remove {
                member: GroupMember::Individual(member),
            } if member == author
        );
        if is_leaving {
            AuthorizationActor::Member(author)
        } else {
            self.authorization_actor()
                .unwrap_or(AuthorizationActor::Member(author))
        }
    }

    /// Auth dependencies.
    fn dependencies(&self) -> Vec<OP>;

    /// The id of the group the action applies to.
    fn group_id(&self) -> ID;

    /// The group action.
    fn action(&self) -> GroupAction<ID, C>;

    /// Returns groups which are dependant on being able to process this action.
    ///
    /// This calculation returns the ids of groups required for processing the action as well as
    /// the target group itself.
    fn required_groups(&self) -> Vec<ID> {
        let mut members = self.action().required_groups();
        members.push(self.group_id());
        members
    }
}
