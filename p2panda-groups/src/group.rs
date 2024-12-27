// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::hash::Hash as StdHash;

use thiserror::Error;

#[derive(Debug)]
pub struct Group<I, M>
where
    I: StdHash + PartialEq + Eq,
    M: StdHash + PartialEq + Eq,
{
    members: HashSet<I>,
    removed_members: HashSet<I>,
    infos: HashMap<I, MemberInfo<I, M>>,
    remove_infos: HashMap<M, RemoveInfo<I>>,
    my_id: I,
    adds_by_msg: HashMap<M, I>,
    removes_by_msg: HashSet<M>,
}

impl<I, M> Group<I, M>
where
    I: Copy + StdHash + PartialEq + Eq,
    M: Copy + StdHash + PartialEq + Eq,
{
    pub fn new(my_id: I) -> Self {
        Self::from_members(&[my_id], my_id)
    }

    pub fn from_members(initial_members: &[I], my_id: I) -> Self {
        let mut infos = HashMap::with_capacity(initial_members.len());
        let mut members = HashSet::with_capacity(initial_members.len());
        for member in initial_members {
            infos.insert(*member, MemberInfo::new(*member, None, initial_members));
            members.insert(*member);
        }

        Self {
            members,
            removed_members: HashSet::new(),
            infos,
            remove_infos: HashMap::new(),
            my_id,
            adds_by_msg: HashMap::new(),
            removes_by_msg: HashSet::new(),
        }
    }

    /// Handles message adding a new member ("added") to the group by another member ("actor").
    ///
    /// Please note that a user can only be added to a group once.
    ///
    /// Returns true if the add was immediately cancelled by a concurrent remove that we have
    /// already processed.
    pub fn add(&mut self, actor: I, added: I, message_id: M) -> Result<bool, GroupError<I, M>> {
        let mut added_info = MemberInfo::new(added, Some(actor), &[]);
        added_info.acks.insert(actor);
        added_info.acks.insert(added);
        added_info.acks.insert(self.my_id);

        // Is `actor` still a member of the group itself?
        let mut removed_by_concurrency = false;
        if self.members.contains(&actor) {
            // @TODO(adz): How to handle adds when the member already exists? This de-duplicates
            // the member, but overwrites the `added_info` with a new state?
            self.members.insert(added);
            self.infos.insert(added, added_info);
        } else {
            // `actor` has been removed by a remove messages concurrent to this add message. All
            // the remove messages removing `actor` get credit for removing `added` as well.
            removed_by_concurrency = true;
            let actor_info = self
                .infos
                .get_mut(&actor)
                .ok_or(GroupError::UnrecognizedMember(actor))?;
            for remove_message_id in &actor_info.remove_messages {
                let remove_info = self
                    .remove_infos
                    .get_mut(remove_message_id)
                    .expect("remove_infos values should be consistent with remove_messages");
                remove_info.removed.insert(added);
                added_info.remove_messages.push(*remove_message_id);
            }
            self.removed_members.insert(added);
        }

        // If `actor` acknowledged adding or removing a member in the past, then we can be sure
        // that `added` also acknowledges it as they must have been made aware of this history by
        // receiving the welcome message from `actor`.
        for member in &self.members {
            let member_info = self
                .infos
                .get_mut(member)
                .expect("infos values should be consistent with members keys");
            if member_info.acks.contains(&actor) {
                member_info.acks.insert(added);
            }
        }

        for member in &self.removed_members {
            let member_info = self
                .infos
                .get_mut(member)
                .expect("infos values should be consistent with removed_members keys");
            if member_info.acks.contains(&actor) {
                member_info.acks.insert(added);
            }
        }

        for message_id in &self.removes_by_msg {
            let remove_info = self
                .remove_infos
                .get_mut(message_id)
                .expect("remove_infos values should be consistent with removes_by_msg keys");
            if remove_info.acks.contains(&actor) {
                remove_info.acks.insert(added);
            }
        }

        self.adds_by_msg.insert(message_id, added);

        Ok(removed_by_concurrency)
    }

    /// Returns the list of group members who were removed by this Remove Message, i.e., the delta
    /// between the resulting list of members and the original list of members.
    ///
    /// This may exclude some members in removed because they were already removed, and it may
    /// include extra members that were removed by concurrency.
    pub fn remove(
        &mut self,
        actor: I,
        removed: &[I],
        message_id: M,
    ) -> Result<Vec<I>, GroupError<I, M>> {
        let mut remove_result = Vec::new();

        let mut remove_info = RemoveInfo::new(removed);
        remove_info.acks.insert(actor);
        remove_info.acks.insert(self.my_id);

        // Remove the users in removed (if needed) and mark them as removed by this message.
        for removed_member in removed {
            let has_removed_member = self.members.remove(removed_member);
            if has_removed_member {
                let member_info = self
                    .infos
                    .get_mut(removed_member)
                    .expect("infos values should be consistent with members keys");
                self.removed_members.insert(*removed_member);
                member_info.remove_messages.push(message_id);
                remove_result.push(*removed_member);
            } else if self.removed_members.contains(removed_member) {
                if let Some(member_info) = self.infos.get_mut(removed_member) {
                    // Member has already been removed.
                    member_info.remove_messages.push(message_id);
                } else {
                    return Err(GroupError::UnrecognizedMember(*removed_member));
                }
            } else {
                return Err(GroupError::UnrecognizedMember(*removed_member));
            }
        }

        self.removes_by_msg.insert(message_id);
        self.remove_infos.insert(message_id, remove_info);

        // If a removed user performed an add concurrent to this message (i.e., not yet ack'd by
        // actor), then the user added by that message is also considered removed by this
        // message. This loop searches for such adds and removes their target.
        //
        // Since users removed in this fashion may themselves have added users, we have to apply
        // this rule repeatedly until it stops making progress.
        //
        // @TODO(adz): Can this be optimized?
        loop {
            let remove_info = self
                .remove_infos
                .get_mut(&message_id)
                .expect("infos values should be consistent with members keys");

            let mut made_progress = false;

            for member in &self.members {
                let member_info = self
                    .infos
                    .get_mut(member)
                    .expect("infos values should be consistent with members keys");
                let contains = member_info
                    .actor
                    .map_or(false, |actor| remove_info.removed.contains(&actor));
                if contains && !member_info.acks.contains(&actor) {
                    remove_result.push(*member);
                    self.removed_members.insert(*member);
                    member_info.remove_messages.push(message_id);
                    remove_info.removed.insert(*member);
                    made_progress = true;
                }
            }

            for removed_member in &remove_result {
                self.members.remove(removed_member);
            }

            // Loop through already removed users, adding this message to their list of remove messages
            // if it applies.
            for member in &self.removed_members {
                let member_info = self
                    .infos
                    .get_mut(member)
                    .expect("infos values should be consistent with members keys");
                let contains = member_info
                    .actor
                    .map_or(false, |actor| remove_info.removed.contains(&actor));
                if contains
                    && member_info.acks.contains(&actor)
                    && member_info.remove_messages.contains(&message_id)
                {
                    remove_info.removed.insert(*member);
                    made_progress = true;
                }
            }

            if !made_progress {
                break;
            }
        }

        Ok(remove_result)
    }

    pub fn ack(&mut self, acker: I, message_id: M) -> Result<(), GroupError<I, M>> {
        let added = self.adds_by_msg.get_mut(&message_id);
        match added {
            Some(added) => {
                let member_info = self
                    .infos
                    .get_mut(added)
                    .expect("adds_by_msg values should be consistent with members keys");
                // Don't complain if its the added user acking themselves (for real this time, as
                // opposed to the implicit ack that they give just from being added).
                if !member_info.acks.insert(acker) && !member_info.id.eq(&acker) {
                    return Err(GroupError::AlreadyAcked);
                }
            }
            None => {
                let remove_info = self.remove_infos.get_mut(&message_id);
                match remove_info {
                    Some(remove_info) => {
                        if !remove_info.acks.insert(acker) {
                            return Err(GroupError::AlreadyAcked);
                        }

                        if remove_info.removed.contains(&acker) {
                            return Err(GroupError::AckingOwnRemoval);
                        }
                    }
                    None => return Err(GroupError::UnknownMessage(message_id)),
                }
            }
        }

        Ok(())
    }

    pub fn size(&self) -> usize {
        self.members.len()
    }

    pub fn members(&self) -> HashSet<I> {
        self.members.clone()
    }

    pub fn members_without_me(&self) -> HashSet<I> {
        let mut set = self.members();
        set.remove(&self.my_id);
        set
    }

    pub fn members_view(&self, viewer: &I) -> HashSet<I> {
        if viewer.eq(&self.my_id) {
            return self.members();
        }

        let mut view = HashSet::new();

        // Include current members whose add was acked by viewer.
        for member in &self.members {
            let member_info = self
                .infos
                .get(member)
                .expect("infos values should be consistent with members keys");
            if member_info.acks.contains(viewer) {
                view.insert(*member);
            }
        }

        // Also include removed members, none of whose removes have been acked by viewer.
        for member in &self.removed_members {
            let member_info = self
                .infos
                .get(member)
                .expect("infos values should be consistent with removed_members keys");
            let any_acked = member_info.remove_messages.iter().any(|message_id| {
                let remove_info = self
                    .remove_infos
                    .get(message_id)
                    .expect("remove_infos values should be consistent with remove_messages");
                remove_info.acks.contains(viewer)
            });
            if !any_acked {
                view.insert(*member);
            }
        }

        view
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemberInfo<I, M>
where
    I: StdHash + PartialEq + Eq,
    M: StdHash + PartialEq + Eq,
{
    pub id: I,

    /// Who added this member.
    pub actor: Option<I>,

    /// Remove messages that removed this member.
    pub remove_messages: Vec<M>,

    /// Users who have ack'd the message.
    pub acks: HashSet<I>,
}

impl<I, M> MemberInfo<I, M>
where
    I: Copy + StdHash + PartialEq + Eq,
    M: Copy + StdHash + PartialEq + Eq,
{
    fn new(id: I, actor: Option<I>, initial_acks: &[I]) -> Self {
        let mut acks = HashSet::with_capacity(initial_acks.len());
        for ack in initial_acks {
            acks.insert(*ack);
        }

        Self {
            id,
            actor,
            remove_messages: Vec::new(),
            acks,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoveInfo<I>
where
    I: StdHash + PartialEq + Eq,
{
    /// Users removed by this message, including users who would have been removed except they were
    /// removed previously.
    pub removed: HashSet<I>,

    /// Users who have ack'd the member.
    pub acks: HashSet<I>,
}

impl<I> RemoveInfo<I>
where
    I: Copy + StdHash + PartialEq + Eq,
{
    pub fn new(removed_members: &[I]) -> Self {
        let mut removed = HashSet::with_capacity(removed_members.len());
        for member in removed_members {
            removed.insert(*member);
        }

        Self {
            removed,
            acks: HashSet::new(),
        }
    }
}

// @TODO: Improve error types and messages.
#[derive(Debug, Error)]
pub enum GroupError<I, M> {
    #[error("tried to access unrecognized member")]
    UnrecognizedMember(I),

    #[error("already acked")]
    AlreadyAcked,

    #[error("member acking their own removal")]
    AckingOwnRemoval,

    #[error("message not recognized")]
    UnknownMessage(M),
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{Group, GroupError, MemberInfo};

    type UserId = &'static str;
    type MessageId = usize;
    type TestGroup = Group<UserId, MessageId>;

    fn generate_groups(members: &[UserId]) -> Vec<TestGroup> {
        let mut groups = Vec::with_capacity(members.len());
        for i in 0..members.len() {
            groups.push(Group::from_members(members, members[i]));
        }
        groups
    }

    fn assert_members_eq(groups: &[TestGroup], members: &[UserId], expected_members: &[UserId]) {
        let mut expected_set = HashSet::new();
        for member in expected_members {
            expected_set.insert(*member);
        }

        for group in groups {
            assert_eq!(group.members(), expected_set);
            for member in members {
                assert_eq!(group.members_view(member), expected_set);
            }
        }
    }

    #[test]
    fn correct_init_views() {
        let members = ["penguin", "icebear", "panda", "llama"];
        let groups = generate_groups(&members);
        assert_members_eq(&groups, &members, &members);
    }

    #[test]
    fn add() {
        // Initial group with members ["llama"].
        let mut group: TestGroup = Group::new("llama");
        assert_eq!(group.size(), 1);

        // Try to add new member "panda" to the group but the adder "grizzly" is not a member
        // itself yet.
        assert!(matches!(
            group.add("grizzly", "panda", 0),
            Err(GroupError::UnrecognizedMember("grizzly"))
        ));

        // Successfully add new member "icebear", added by "llama".
        group.add("llama", "icebear", 0).unwrap();
        assert_eq!(group.size(), 2);

        // Added member and actor should both be acknowledged by each other.
        assert_eq!(
            group.infos.get(&"llama"),
            Some(&MemberInfo {
                id: "llama",
                actor: None,
                remove_messages: vec![],
                acks: ["llama", "icebear"].into(),
            }),
        );
        assert_eq!(
            group.infos.get(&"icebear"),
            Some(&MemberInfo {
                id: "icebear",
                actor: Some("llama"),
                remove_messages: vec![],
                acks: ["llama", "icebear"].into(),
            }),
        );

        // "llama" adds "panda" now as well.
        group.add("llama", "panda", 1).unwrap();

        // That "llama" added "icebear" in the past should also be acknowledged by "panda" now as
        // this happened in the past and they should be aware of it.
        assert_eq!(
            group.infos.get(&"icebear"),
            Some(&MemberInfo {
                id: "icebear",
                actor: Some("llama"),
                remove_messages: vec![],
                acks: ["llama", "icebear", "panda"].into(),
            }),
        );
    }

    #[test]
    fn add_acks() {
        let initial_members = ["icebear", "grizzly", "turtle", "penguin"];
        let add_message_id = 0;
        let mut groups = generate_groups(&initial_members);

        // "panda" is added to everyone's group by "icebear".
        for group in groups.iter_mut() {
            group.add("icebear", "panda", add_message_id).unwrap();
        }
        let members_with_added = ["icebear", "grizzly", "turtle", "penguin", "panda"];

        for group in &groups {
            for member in initial_members {
                if member == group.my_id || member == "icebear" {
                    // Everyone maintaining their own group acknowledged the newly added member and
                    // the actor ("icebear") itself.
                    assert_eq!(group.members_view(&member), members_with_added.into());
                } else {
                    // .. from the perspective of all the others we didn't receive an acknowledment
                    // yet so we still consider them "not added".
                    assert_eq!(group.members_view(&member), initial_members.into());
                }
            }
        }

        // Everyone acknowledges each other's addition of "panda".
        for group in groups.iter_mut() {
            for member in initial_members {
                if member == group.my_id || member == "icebear" {
                    continue;
                }

                group.ack(&member, add_message_id).unwrap();
            }
        }

        // Now everyone should have the same members state.
        assert_members_eq(&groups, &initial_members, &members_with_added);
    }

    #[test]
    fn remove_acks() {
        let initial_members = ["icebear", "grizzly", "turtle", "penguin", "panda"];
        let remove_message_id = 0;
        let mut groups = generate_groups(&initial_members);

        // "icebear" removes "panda", everyone but "panda" receives the remove message.
        for group in groups.iter_mut() {
            if group.my_id == "panda" {
                continue;
            }
            group
                .remove("icebear", &["panda"], remove_message_id)
                .unwrap();
        }
        let members_after_removal = ["icebear", "grizzly", "turtle", "penguin"];

        // At this point, everyone thinks they and the remover "icebear" have processed the remove
        // of "panda", except of "panda" itself, while no one else has.
        for group in &groups {
            for member in initial_members {
                if member != "panda" && member == group.my_id {
                    assert_eq!(group.members(), members_after_removal.into());
                } else {
                    if member == "icebear" && group.my_id != "panda" {
                        assert_eq!(group.members_view(&member), members_after_removal.into());
                    } else {
                        assert_eq!(group.members_view(&member), initial_members.into());
                    }
                }
            }
        }

        // Everyone acknowledges each other's removal of "panda".
        for group in groups.iter_mut() {
            for member in members_after_removal {
                if member == group.my_id || member == "icebear" || group.my_id == "panda" {
                    continue;
                }

                group.ack(&member, remove_message_id).unwrap();
            }
        }

        // Now everyone should have the same members state.
        assert_members_eq(
            &groups[0..4],
            &members_after_removal,
            &members_after_removal,
        );

        // .. except of "panda"!
        assert_eq!(groups[4].members(), initial_members.into());
    }
}
