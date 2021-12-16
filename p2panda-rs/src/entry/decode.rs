// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;

use arrayvec::ArrayVec;
use bamboo_rs_core_ed25519_yasmf::Entry as BambooEntry;

use crate::entry::{Entry, EntrySigned, EntrySignedError, LogId, SeqNum, SIGNATURE_SIZE};
use crate::hash::{Hash, HASH_SIZE};
use crate::operation::{Operation, OperationEncoded};

/// Takes [`EntrySigned`] and optionally [`OperationEncoded`] as arguments, returns a decoded and
/// unsigned [`Entry`]. When a [`OperationEncoded`] is passed it will automatically check its
/// integrity with this [`Entry`] by comparing their hashes. Valid operations will be included in the
/// returned [`Entry`], if an invalid operation is passed an error will be returned.
///
/// Entries are separated from the operations they refer to. Since operations can independently be
/// deleted they can be passed on as an optional argument. When a [`Operation`] is passed it will
/// automatically check its integrity with this Entry by comparing their hashes.
pub fn decode_entry(
    entry_encoded: &EntrySigned,
    operation_encoded: Option<&OperationEncoded>,
) -> Result<Entry, EntrySignedError> {
    // Convert to Entry from bamboo_rs_core_ed25519_yasmf first
    let entry: BambooEntry<ArrayVec<[u8; HASH_SIZE]>, ArrayVec<[u8; SIGNATURE_SIZE]>> =
        entry_encoded.into();

    let operation = match operation_encoded {
        Some(msg) => {
            entry_encoded.validate_operation(msg)?;
            Some(Operation::from(msg))
        }
        None => None,
    };

    let entry_hash_backlink: Option<Hash> = match entry.backlink {
        Some(link) => Some(link.try_into()?),
        None => None,
    };

    let entry_hash_skiplink: Option<Hash> = match entry.lipmaa_link {
        Some(link) => Some(link.try_into()?),
        None => None,
    };

    Ok(Entry::new(
        &LogId::new(entry.log_id as i64),
        operation.as_ref(),
        entry_hash_skiplink.as_ref(),
        entry_hash_backlink.as_ref(),
        &SeqNum::new(entry.seq_num as i64).unwrap(),
    )
    .unwrap())
}
