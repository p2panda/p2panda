use std::collections::BTreeMap;

use crate::hash::Hash;
use crate::identity::Author;
use crate::instance::Instance;

struct Membership {
    author: Author,
    key_group: Instance,
    accepted: bool,
    can_authorise: bool,
    can_create: bool,
    can_update: bool,
    can_delete: bool
}

struct KeyGroupIndex {
    members: Vec<Membership>,
    by_key_group: BTreeMap<Hash, Vec<Membership>>,
    by_author: BTreeMap<Hash, Vec<Membership>
}

impl KeyGroup {
    fn new() -> Self {

    }
}
