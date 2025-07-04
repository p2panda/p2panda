// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cmp::Ordering;
use std::fmt::Display;

/// The four basic access levels which can be assigned to an actor. Greater access levels are
/// assumed to also contain all lower ones.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessLevel {
    /// Permission to sync a data set.
    Pull,

    /// Permission to read a data set.
    Read,

    /// Permission to write to a data set.
    Write,

    /// Permission to apply membership changes to a group.
    Manage,
}

/// A level of access with optional conditions which can be assigned to an actor.
///
/// Access can be used to understand the rights of an actor to perform actions (request data,
/// write data, etc..) within a certain data set. Custom conditions can be defined by the user in
/// order to introduce domain specific access boundaries or integrate with another access token.
///
/// For example, a condition to model access boundaries using paths could be introduced where
/// having access to "/public" gives you access to "/public/stuff" and "/public/other/stuff" but
/// not "/private" or "/private/stuff".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access<C = ()> {
    pub conditions: Option<C>,
    pub level: AccessLevel,
}

impl<C> Display for Access<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self.level {
            AccessLevel::Pull => "pull",
            AccessLevel::Read => "read",
            AccessLevel::Write => "write",
            AccessLevel::Manage => "manage",
        };

        write!(f, "{}", s)
    }
}

impl<C> Access<C> {
    /// Pull access level.
    pub fn pull() -> Self {
        Self {
            level: AccessLevel::Pull,
            conditions: None,
        }
    }

    /// Read access level.
    pub fn read() -> Self {
        Self {
            level: AccessLevel::Read,
            conditions: None,
        }
    }

    /// Write access level.
    pub fn write() -> Self {
        Self {
            level: AccessLevel::Write,
            conditions: None,
        }
    }

    /// Manage access level.
    pub fn manage() -> Self {
        Self {
            level: AccessLevel::Manage,
            conditions: None,
        }
    }

    /// Attach conditions to an access level.
    pub fn with_conditions(mut self, conditions: C) -> Self {
        self.conditions = Some(conditions);
        self
    }

    /// Access level is Pull.
    pub fn is_pull(&self) -> bool {
        matches!(self.level, AccessLevel::Pull)
    }

    /// Access level is Read.
    pub fn is_read(&self) -> bool {
        matches!(self.level, AccessLevel::Read)
    }

    /// Access level is Write.
    pub fn is_write(&self) -> bool {
        matches!(self.level, AccessLevel::Write)
    }

    /// Access level is Manage.
    pub fn is_manage(&self) -> bool {
        matches!(self.level, AccessLevel::Manage)
    }
}

impl<C: PartialOrd> PartialOrd for Access<C> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self.conditions.as_ref(), other.conditions.as_ref()) {
            // If self and other contain conditions compare them first.
            (Some(self_cond), Some(other_cond)) => {
                match self_cond.partial_cmp(other_cond) {
                    // When conditions are equal or greater then fall back to comparing the access
                    // level.
                    Some(Ordering::Greater | Ordering::Equal) => {
                        match self.level.cmp(&other.level) {
                            Ordering::Less => Some(Ordering::Less),
                            Ordering::Equal | Ordering::Greater => Some(Ordering::Greater),
                        }
                    }
                    Some(Ordering::Less) => Some(Ordering::Less),
                    None => None,
                }
            }
            (None, Some(_)) => match self.level.cmp(&other.level) {
                Ordering::Less => Some(Ordering::Less),
                Ordering::Equal | Ordering::Greater => Some(Ordering::Greater),
            },
            _ => Some(self.level.cmp(&other.level)),
        }
    }
}

impl<C: PartialOrd + Eq> Ord for Access<C> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Less)
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use crate::Access;

    /// Conditions which models access based on paths. Having access to "/public" gives you access
    /// to "/public/stuff" and "/public/other/stuff" but not "/private" or "/private/stuff".
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct PathCondition(String);

    impl PartialOrd for PathCondition {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            let self_parts: Vec<_> = self.0.split('/').filter(|s| !s.is_empty()).collect();
            let other_parts: Vec<_> = other.0.split('/').filter(|s| !s.is_empty()).collect();

            let min_len = self_parts.len().min(other_parts.len());
            let is_prefix = self_parts[..min_len] == other_parts[..min_len];

            if is_prefix {
                match self_parts.len().cmp(&other_parts.len()) {
                    Ordering::Less => Some(Ordering::Greater),
                    Ordering::Equal => Some(Ordering::Equal),
                    Ordering::Greater => Some(Ordering::Less),
                }
            } else {
                None
            }
        }
    }

    #[test]
    fn path_condition_comparators() {
        let root_access = Access::read().with_conditions(PathCondition("/root".to_string()));
        let private_access =
            Access::read().with_conditions(PathCondition("/root/private".to_string()));
        let public_access =
            Access::read().with_conditions(PathCondition("/root/public".to_string()));

        // Access to "/root" gives access to all sub-paths
        assert!(root_access >= private_access);
        assert!(root_access >= public_access);

        // Unrelated paths are not comparable.
        assert!(!(private_access >= public_access));
        assert!(!(private_access <= public_access));

        let read_access_to_root =
            Access::read().with_conditions(PathCondition("/root".to_string()));
        let requested_write_access_to_sub_path =
            Access::write().with_conditions(PathCondition("/root/private".to_string()));

        assert!(requested_write_access_to_sub_path < read_access_to_root);

        let unconditional_read = Access::<PathCondition>::read();
        assert!(unconditional_read > public_access);
    }

    /// Conditions containing an access expiry timestamp.
    #[derive(Debug, Clone, PartialOrd, PartialEq, Eq)]
    struct ExpiryTimestamp(u64);

    #[test]
    fn expiry_timestamp_access_ordering() {
        let access_expires_soon = Access::read().with_conditions(ExpiryTimestamp(10));
        let access_expires_later = Access::read().with_conditions(ExpiryTimestamp(100));

        // access_expires_later grants more access (access valid for longer).
        assert!(access_expires_later > access_expires_soon);

        // access_expires_soon grants less access (access valid for shorter time).
        assert!(access_expires_soon < access_expires_later);

        // It's likely access levels will be tested against some kind of request, here we
        // construct a request that requires that the requestor has access equal or greater than
        // "Read" which expires at timestamp 50.
        const NOW: ExpiryTimestamp = ExpiryTimestamp(50);
        let requested_read_access = Access::read().with_conditions(NOW);

        // This access has already expired, it is less than the requested access, and the request
        // would be rejected.
        assert!(access_expires_soon < requested_read_access);

        // This access is still valid, it is greater than the requested access, and the request
        // would be accepted.
        assert!(access_expires_later >= requested_read_access);

        // Even though the held access level (Read) is greater than the requested access level (Pull)
        // the condition has expired and so the held access is still less than the requested and
        // the request would be rejected.
        let requested_pull_access = Access::pull().with_conditions(NOW);
        assert!(access_expires_soon < requested_pull_access);

        // On the other hand, if the condition is still valid, but the requested access level is
        // greater than the held one, the request will still be rejected.
        let requested_write_access = Access::write().with_conditions(NOW);
        assert!(access_expires_later < requested_write_access);

        // An access level without an expiry is greater or equal than one with.
        let requested_read_access = Access::read().with_conditions(NOW);
        let access_no_expiry = Access::<ExpiryTimestamp>::read();
        assert!(access_no_expiry > requested_read_access);
    }
}
