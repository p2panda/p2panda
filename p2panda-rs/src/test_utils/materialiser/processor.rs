// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::test_utils::logs::LogEntry;
use crate::test_utils::materialiser::Materialiser;
use crate::test_utils::node::utils::PERMISSIONS_SCHEMA_HASH;

/// Get all permission entries from this author
pub fn author_permission_entries(entries: &Vec<LogEntry>, author: &String) -> Vec<LogEntry> {
    entries
        .iter()
        .filter(|log_entry| {
            &log_entry.author() == author
                && log_entry.message().schema().as_str() == PERMISSIONS_SCHEMA_HASH
        })
        .map(|log_entry| log_entry.to_owned())
        .collect()
}

/// Filter entries against Instance permissions
pub fn filter_entries(entries: Vec<LogEntry>) -> Vec<LogEntry> {
    let mut filtered_entries = Vec::new();
    let mut materialiser = Materialiser::new();

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Check permissions
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    for entry in &entries {
        // Materialise permissions entries by this author
        let author_permissions_entries =
            author_permission_entries(&entries, &entry.instance_author());
        let author_permissions = materialiser
            .materialise(&author_permissions_entries)
            .unwrap();

        // If the author of this message is the Instance author then they don't need permissions
        // or if this is a create message, you don't need permissions
        if entry.author() == entry.instance_author() || entry.message().is_create() {
            filtered_entries.push(entry.clone().to_owned());
        // Otherwise we need to check if the author has been given permissions
        } else {
            // Loop over all permissions given by this author
            let permissions_instances =
                author_permissions.get(&PERMISSIONS_SCHEMA_HASH.to_string());

            if permissions_instances.is_some() {
                for (_instance_id, message_fields) in permissions_instances.unwrap() {
                    // Extract permitted author from message fields
                    let permitted_author = match message_fields.get("author") {
                        Some(message_value) => match message_value {
                            crate::message::MessageValue::Text(str) => str,
                            _ => todo!(),
                        },
                        None => todo!(),
                    };
                    // Extract permitted instance from message fields
                    let permitted_instance = match message_fields.get("id") {
                        Some(message_value) => match message_value {
                            crate::message::MessageValue::Text(str) => str,
                            _ => todo!(),
                        },
                        None => todo!(),
                    };

                    // Check if author of this entry has been given permissions for this Instance
                    if entry.message().id().unwrap().as_str() == permitted_instance
                        && entry.author() == permitted_author.to_owned()
                    {
                        filtered_entries.push(entry.clone().to_owned());
                    } else {
                        continue;
                    }
                }
            }
        }
    }
    filtered_entries
}
