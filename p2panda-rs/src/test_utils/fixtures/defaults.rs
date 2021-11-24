// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods for generating some useful default values without any passed parameters. These are used 
//! when composing test templates where default fixtures can't be injected in the usual way.

use crate::test_utils::fixtures;
use crate::hash::Hash;
use crate::message::Message;
use crate::entry::Entry;
use crate::test_utils::utils::{DEFAULT_HASH, DEFAULT_SCHEMA_HASH};

/// The default hash as an option
pub fn some_hash() -> Option<Hash> {
    fixtures::some_hash(DEFAULT_HASH)
}

/// The default CREATE message
pub fn create_message() -> Message {
    fixtures::create_message(
        fixtures::schema(DEFAULT_SCHEMA_HASH),
        fixtures::fields(vec![("message", "Hello!")]),
    )
}

/// The default UPDATE message
pub fn update_message() -> Message {
    fixtures::update_message(
        fixtures::schema(DEFAULT_SCHEMA_HASH),
        fixtures::hash(DEFAULT_HASH),
        fixtures::fields(vec![("message", "Updated, hello!")]))
}

/// The default DELETE message
pub fn delete_message() -> Message {
    fixtures::delete_message(
        fixtures::schema(DEFAULT_SCHEMA_HASH),
        fixtures::hash(DEFAULT_HASH),
    )
}

/// The default first entry
pub fn first_entry() -> Entry {
    fixtures::entry(create_message(), fixtures::seq_num(1), None, None)
}

/// The default entry with only a backlink
pub fn entry_with_backlink() -> Entry {
    fixtures::entry(
        create_message(),
        fixtures::seq_num(2),
        some_hash(),
        None,
    )
}

/// The default entry with a backlink and skiplink
pub fn entry_with_backlink_and_skiplink() -> Entry {
    fixtures::entry(
        create_message(),
        fixtures::seq_num(13),
        some_hash(),
        some_hash(),
    )
}   

/// The default entry with a skiplink and no backlink
pub fn entry_with_only_a_skiplink() -> Entry {
    fixtures::entry(
        create_message(),
        fixtures::seq_num(13),
        some_hash(),
        some_hash(),
    )
}   
