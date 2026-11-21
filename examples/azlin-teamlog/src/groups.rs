//! Groups of the team log: the department tree, who is in which group, and the subtree queries a
//! scope is made of (planning/architecture.md section 7.2 "Groups nest (a department tree)", section
//! 5.3 `reg_group` and `reg_group_member`; build unit S17, lane W1-S17c1).
//!
//! A group has at most one parent; a group without a parent is a top-level group, and a top-level
//! group never moves (authority table v0). "In scope" is a question about this tree: a group admin's
//! scope is the SUBTREE of its `admin_scope` group, and a member is in scope when it belongs, directly
//! or through nesting, to a group of that subtree. Every walk up the tree is bounded by
//! `MAX_GROUP_DEPTH`; a chain that is longer (a state that no entry can have made) is read as "not in
//! the subtree", so a damaged tree grants no scope.

use std::collections::{BTreeMap, BTreeSet};

use azlin_seams::{ChainSeq, GroupId, MemberId};

use crate::state::MAX_GROUP_DEPTH;

/// One group, as the team log defines it: the columns of `reg_group` that the log writes (the
/// group key's epoch is the key lanes' own).
///
/// Implements planning/architecture.md section 5.3 (`reg_group`: `group_id`, `parent`, `name_sealed`,
/// `log_seq`).
///
/// Caller: `Groups`, `RegistryDelta::Group`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// The id of the group.
    pub id: GroupId,
    /// The parent group; `None` for a top-level group.
    pub parent: Option<GroupId>,
    /// The group's name, sealed under the account's seal key: the `sealed` field of the
    /// `group_create` entry.
    pub name_sealed: Option<Vec<u8>>,
    /// The position of the newest entry that touched the group.
    pub log_seq: ChainSeq,
}

/// The groups of the account and their members: the tree, and both directions of the membership
/// relation, kept in step by the mutators (every one is `pub(crate)`: only `apply` changes them).
///
/// Implements planning/architecture.md section 7.2 (groups nest; admin roles are scoped to a group
/// subtree) and section 5.3 (`reg_group`, `reg_group_member`).
///
/// Caller: `TeamState::groups`, `admit.rs` (scope, cycles), `apply.rs` (`group_create`,
/// `group_member`, `group_move`, `member_remove`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    groups: BTreeMap<GroupId, Group>,
    members: BTreeMap<GroupId, BTreeSet<MemberId>>,
    memberships: BTreeMap<MemberId, BTreeSet<GroupId>>,
}

impl Groups {
    /// No groups.
    pub fn new() -> Groups {
        Groups::default()
    }

    /// How many groups there are.
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// Whether there is no group.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// The group with this id.
    pub fn get(&self, group: &GroupId) -> Option<&Group> {
        self.groups.get(group)
    }

    /// Whether a group with this id exists.
    pub fn contains(&self, group: &GroupId) -> bool {
        self.groups.contains_key(group)
    }

    /// Every group, in the order of the ids.
    pub fn iter(&self) -> impl Iterator<Item = &Group> {
        self.groups.values()
    }

    /// The members of a group (directly, not through nesting); `None` for an unknown group.
    pub fn members_of(&self, group: &GroupId) -> Option<&BTreeSet<MemberId>> {
        self.members.get(group)
    }

    /// The groups a member belongs to directly; `None` when it belongs to none.
    pub fn groups_of(&self, member: &MemberId) -> Option<&BTreeSet<GroupId>> {
        self.memberships.get(member)
    }

    /// Every (group, member) pair, in order.
    pub fn membership_pairs(&self) -> Vec<(GroupId, MemberId)> {
        self.members
            .iter()
            .flat_map(|(group, members)| members.iter().map(move |member| (*group, *member)))
            .collect()
    }

    /// The number of ancestors of a group (a top-level group has 0); `None` for an unknown group and
    /// for a chain longer than `MAX_GROUP_DEPTH` (a damaged tree).
    pub fn depth_of(&self, group: &GroupId) -> Option<usize> {
        let mut current = *group;
        let mut depth = 0usize;
        loop {
            let parent = self.groups.get(&current)?.parent;
            match parent {
                None => return Some(depth),
                Some(parent) => {
                    depth = depth.checked_add(1)?;
                    if depth > MAX_GROUP_DEPTH {
                        return None;
                    }
                    current = parent;
                }
            }
        }
    }

    /// Whether `group` lies in the subtree of `root`: it is `root` or has `root` among its ancestors.
    /// False for an unknown `group`, and for a chain longer than `MAX_GROUP_DEPTH`.
    ///
    /// Implements planning/architecture.md section 7.2 (admin roles are scoped to a group subtree).
    ///
    /// Caller: `member_in_subtree`, `admit.rs` (`in_scope`, the cycle check of `group_move`).
    pub fn is_in_subtree(&self, group: &GroupId, root: &GroupId) -> bool {
        let mut current = *group;
        for _ in 0..=MAX_GROUP_DEPTH {
            let Some(found) = self.groups.get(&current) else {
                return false;
            };
            if current == *root {
                return true;
            }
            match found.parent {
                Some(parent) => current = parent,
                None => return false,
            }
        }
        false
    }

    /// Whether the member belongs, directly or through nesting, to a group of the subtree of `root`.
    ///
    /// Implements planning/architecture.md section 7.2 ("In scope = the subject member belongs,
    /// directly or through nesting, to a group of the signer's subtree": the authority table v0).
    ///
    /// Caller: `admit.rs` (`in_scope`).
    pub fn member_in_subtree(&self, member: &MemberId, root: &GroupId) -> bool {
        match self.memberships.get(member) {
            Some(groups) => groups.iter().any(|group| self.is_in_subtree(group, root)),
            None => false,
        }
    }

    /// The groups from the top of the tree down to `group`, `group` last; `None` for an unknown group
    /// or a damaged tree. The patches of these groups, in this order, are what `azlin_policy::effective`
    /// walks.
    ///
    /// Implements planning/architecture.md section 5.10 ("groups inherit and may only tighten").
    ///
    /// Caller: `TeamState::effective_policy_of`, `admit.rs` and `apply.rs` (`policy_set`).
    pub fn path_from_top(&self, group: &GroupId) -> Option<Vec<GroupId>> {
        let mut path = vec![*group];
        let mut current = *group;
        loop {
            let parent = self.groups.get(&current)?.parent;
            match parent {
                None => break,
                Some(parent) => {
                    if path.len() > MAX_GROUP_DEPTH {
                        return None;
                    }
                    path.push(parent);
                    current = parent;
                }
            }
        }
        path.reverse();
        Some(path)
    }

    /// Every group of the subtree of `root`, `root` included (empty when `root` is unknown).
    ///
    /// Implements planning/architecture.md section 7.2 (the subtree of a scope group).
    ///
    /// Caller: the tests and the differential tests of S47a (`subtree_of_nested_groups_is_transitive`).
    pub fn subtree(&self, root: &GroupId) -> BTreeSet<GroupId> {
        self.groups
            .keys()
            .filter(|group| self.is_in_subtree(group, root))
            .copied()
            .collect()
    }

    /// The number of levels below `group` (0 for a group with no child): the depth a move adds.
    /// `None` for an unknown group.
    ///
    /// Implements planning/architecture.md section 1.4 rule 4 (the depth of the tree is bounded).
    ///
    /// Caller: `admit.rs` (`group_move` refuses a move that would make the tree deeper than
    /// `MAX_GROUP_DEPTH`).
    pub fn height_below(&self, group: &GroupId) -> Option<usize> {
        if !self.groups.contains_key(group) {
            return None;
        }
        let base = self.depth_of(group)?;
        let mut deepest = base;
        for other in self.groups.keys() {
            if other != group && self.is_in_subtree(other, group) {
                deepest = deepest.max(self.depth_of(other)?);
            }
        }
        deepest.checked_sub(base)
    }

    /// A group, to change it in a test.
    #[cfg(test)]
    pub(crate) fn get_mut(&mut self, group: &GroupId) -> Option<&mut Group> {
        self.groups.get_mut(group)
    }

    /// Adds a group; `false` (and no change) when the id is taken.
    pub(crate) fn insert(&mut self, group: Group) -> bool {
        if self.groups.contains_key(&group.id) {
            return false;
        }
        self.members.insert(group.id, BTreeSet::new());
        self.groups.insert(group.id, group);
        true
    }

    /// Moves a group under a new parent; `false` (and no change) for an unknown group.
    pub(crate) fn set_parent(
        &mut self,
        group: &GroupId,
        parent: Option<GroupId>,
        log_seq: ChainSeq,
    ) -> bool {
        match self.groups.get_mut(group) {
            Some(found) => {
                found.parent = parent;
                found.log_seq = log_seq;
                true
            }
            None => false,
        }
    }

    /// Puts a member into a group; `false` (and no change) for an unknown group.
    pub(crate) fn add_member(&mut self, group: &GroupId, member: MemberId) -> bool {
        match self.members.get_mut(group) {
            Some(members) => {
                members.insert(member);
                self.memberships.entry(member).or_default().insert(*group);
                true
            }
            None => false,
        }
    }

    /// Takes a member out of a group; `false` (and no change) for an unknown group.
    pub(crate) fn remove_member(&mut self, group: &GroupId, member: &MemberId) -> bool {
        match self.members.get_mut(group) {
            Some(members) => {
                members.remove(member);
                let now_empty = match self.memberships.get_mut(member) {
                    Some(groups) => {
                        groups.remove(group);
                        groups.is_empty()
                    }
                    None => false,
                };
                if now_empty {
                    self.memberships.remove(member);
                }
                true
            }
            None => false,
        }
    }

    /// Takes a member out of every group; the groups it was in, in order.
    pub(crate) fn remove_all_memberships(&mut self, member: &MemberId) -> Vec<GroupId> {
        let groups: Vec<GroupId> = match self.memberships.remove(member) {
            Some(groups) => groups.into_iter().collect(),
            None => Vec::new(),
        };
        for group in &groups {
            if let Some(members) = self.members.get_mut(group) {
                members.remove(member);
            }
        }
        groups
    }
}
