// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;

use bamboo_rs_core_ed25519_yasmf::decode;

use crate::entry::{Entry, EntrySigned, EntrySignedError, SeqNum};
use crate::hash::Hash;
use crate::operation::EncodedOperation;

/// Method to decode an entry and optionally its payload.
///
/// Takes [`EntrySigned`] and optionally [`EncodedOperation`] as arguments, returns a decoded and
/// unsigned [`Entry`].
///
/// Entries are separated from the operations they refer to and serve as "off-chain data". Since
/// operations can independently be deleted they have to be passed on as an optional argument.
///
/// When a [`EncodedOperation`] is passed it will automatically check its integrity with this
/// [`Entry`] by comparing their hashes. Valid operations will be included in the returned
/// [`Entry`], if an invalid operation is passed an error will be returned.
pub fn decode_entry(
    entry_encoded: &EntrySigned,
    operation_encoded: Option<&EncodedOperation>,
) -> Result<Entry, EntrySignedError> {
    let entry = decode(&entry_encoded.to_bytes())?;

    let entry_hash_backlink: Option<Hash> = match entry.backlink {
        Some(link) => Some(link.try_into()?),
        None => None,
    };

    let entry_hash_skiplink: Option<Hash> = match entry.lipmaa_link {
        Some(link) => Some(link.try_into()?),
        None => None,
    };

    let payload_hash: Hash = entry.payload_hash.try_into()?;

    let entry = Entry {
        log_id: entry.log_id.into(),
        entry_hash_backlink,
        entry_hash_skiplink,
        seq_num: SeqNum::new(entry.seq_num)?,
        signature: entry
            .sig
            .ok_or_else(|| EntrySignedError::OperationHashMismatch)?
            .into(),
        payload: operation_encoded.cloned(),
        payload_size: entry.payload_size,
        payload_hash,
    };

    Ok(entry)
}
