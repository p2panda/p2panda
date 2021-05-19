use std::convert::TryInto;

use arrayvec::ArrayVec;
use bamboo_rs_core::Entry as BambooEntry;

use crate::entry::{Entry, EntrySigned, EntrySignedError, LogId, SeqNum};
use crate::hash::Hash;
use crate::message::{Message, MessageEncoded};

/// Takes [`EntrySigned`] and optionally [`MessageEncoded`] as arguments, returns a decoded and
/// unsigned [`Entry`]. When a [`MessageEncoded`] is passed it will automatically check its
/// integrity with this [`Entry`] by comparing their hashes. Valid messages will be included in the
/// returned [`Entry`], if an invalid message is passed an error will be returned.
///
/// Entries are separated from the messages they refer to. Since messages can independently be
/// deleted they can be passed on as an optional argument. When a [`Message`] is passed it will
/// automatically check its integrity with this Entry by comparing their hashes.
pub fn decode_entry(
    entry_encoded: &EntrySigned,
    message_encoded: Option<&MessageEncoded>,
) -> Result<Entry, EntrySignedError> {
    // Convert to Entry from bamboo_rs_core first
    let entry: BambooEntry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> = entry_encoded.into();

    let message = match message_encoded {
        Some(msg) => Some(Message::from(&entry_encoded.validate_message(msg)?)),
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
        message.as_ref(),
        entry_hash_skiplink.as_ref(),
        entry_hash_backlink.as_ref(),
        &SeqNum::new(entry.seq_num as i64).unwrap(),
    )
    .unwrap())
}
