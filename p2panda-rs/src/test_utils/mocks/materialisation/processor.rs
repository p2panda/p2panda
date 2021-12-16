// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::test_utils::mocks::logs::LogEntry;
use crate::test_utils::mocks::materialisation::Materialiser;
use crate::test_utils::mocks::utils::PERMISSIONS_SCHEMA_HASH;

/// Get all permission entries from this author
pub fn author_permission_entries(entries: &[LogEntry], author: &str) -> Vec<LogEntry> {
    entries
        .iter()
        .filter(|log_entry| {
            log_entry.author() == author
                && log_entry.operation().schema().as_str() == PERMISSIONS_SCHEMA_HASH
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

        // If the author of this operation is the Instance author then they don't need permissions
        // or if this is a create operation, you don't need permissions
        if entry.author() == entry.instance_author() || entry.operation().is_create() {
            filtered_entries.push(entry.clone().to_owned());
        // Otherwise we need to check if the author has been given permissions
        } else {
            // Loop over all permissions given by this author
            let permissions_instances =
                author_permissions.get(&PERMISSIONS_SCHEMA_HASH.to_string());

            if let Some(instances) = permissions_instances {
                for operation_fields in instances.values() {
                    // Extract permitted author from operation fields
                    let permitted_author = match operation_fields.get("author") {
                        Some(operation_value) => match operation_value {
                            crate::operation::OperationValue::Text(str) => str,
                            crate::operation::OperationValue::Boolean(_) => todo!(),
                            crate::operation::OperationValue::Integer(_) => todo!(),
                            crate::operation::OperationValue::Float(_) => todo!(),
                            crate::operation::OperationValue::Relation(_) => todo!(),
                        },
                        None => todo!(),
                    };
                    // Extract permitted instance from operation fields
                    let permitted_instance = match operation_fields.get("id") {
                        Some(operation_value) => match operation_value {
                            crate::operation::OperationValue::Text(str) => str,
                            crate::operation::OperationValue::Boolean(_) => todo!(),
                            crate::operation::OperationValue::Integer(_) => todo!(),
                            crate::operation::OperationValue::Float(_) => todo!(),
                            crate::operation::OperationValue::Relation(_) => todo!(),
                        },
                        None => todo!(),
                    };

                    // Check if author of this entry has been given permissions for this Instance
                    if entry.operation().id().unwrap().as_str() == permitted_instance
                        && entry.author() == *permitted_author
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
