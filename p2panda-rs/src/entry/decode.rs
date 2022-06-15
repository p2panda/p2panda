// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;

use arrayvec::ArrayVec;
use bamboo_rs_core_ed25519_yasmf::Entry as BambooEntry;

use crate::entry::{Entry, EntrySigned, EntrySignedError, LogId, SeqNum, SIGNATURE_SIZE};
use crate::hash::{Hash, HASH_SIZE};
use crate::operation::{Operation, OperationEncoded};
use crate::schema::Schema;

/// Method to decode an entry and optionally its payload.
///
/// Takes [`EntrySigned`] and optionally [`OperationEncoded`] as arguments, returns a decoded and
/// unsigned [`Entry`].
///
/// Entries are separated from the operations they refer to and serve as "off-chain data". Since
/// operations can independently be deleted they have to be passed on as an optional argument.
///
/// When a [`OperationEncoded`] is passed it will automatically check its integrity with this
/// [`Entry`] by comparing their hashes. Valid operations will be included in the returned
/// [`Entry`], if an invalid operation is passed an error will be returned.
pub fn decode_entry(
    entry_encoded: &EntrySigned,
    operation_encoded: Option<&OperationEncoded>,
    schema: Option<&Schema>,
) -> Result<Entry, EntrySignedError> {
    let entry: BambooEntry<ArrayVec<[u8; HASH_SIZE]>, ArrayVec<[u8; SIGNATURE_SIZE]>> =
        entry_encoded.into();

    let operation = match operation_encoded {
        Some(payload) => {
            entry_encoded.validate_operation(payload)?;
            Some(payload.decode(schema.ok_or(EntrySignedError::SchemaMissing)?)?)
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
        &LogId::new(entry.log_id),
        operation.as_ref(),
        entry_hash_skiplink.as_ref(),
        entry_hash_backlink.as_ref(),
        &SeqNum::new(entry.seq_num).unwrap(),
    )
    .unwrap())
}
