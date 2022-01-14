// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use bamboo_rs_core_ed25519_yasmf::entry::MAX_ENTRY_SIZE;
use bamboo_rs_core_ed25519_yasmf::{Entry as BambooEntry, Signature as BambooSignature};

use crate::entry::{Entry, EntrySigned, EntrySignedError};
use crate::identity::KeyPair;
use crate::operation::OperationEncoded;

/// Takes an [`Entry`] and a [`KeyPair`], returns signed and encoded entry bytes in form of an
/// [`EntrySigned`] instance.
///
/// After conversion the result is ready to be sent to a p2panda node.
///
/// ## Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use p2panda_rs::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
/// use p2panda_rs::hash::Hash;
/// use p2panda_rs::identity::KeyPair;
/// use p2panda_rs::operation::{Operation, OperationFields, OperationValue};
///
/// // Generate Ed25519 key pair to sign entry with
/// let key_pair = KeyPair::new();
///
/// // Create operation
/// let schema_hash =
///     Hash::new("0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b")?;
/// let mut fields = OperationFields::new();
/// fields.add("title", OperationValue::Text("Hello, Panda!".to_owned()))?;
/// let operation = Operation::new_create(schema_hash, fields)?;
///
/// // Create entry
/// let entry = Entry::new(
///     &LogId::default(),
///     Some(&operation),
///     None,
///     None,
///     &SeqNum::new(1)?,
/// )?;
///
/// // Sign and encode entry
/// let entry_signed_encoded = sign_and_encode(&entry, &key_pair)?;
/// # Ok(())
/// # }
/// ```
pub fn sign_and_encode(entry: &Entry, key_pair: &KeyPair) -> Result<EntrySigned, EntrySignedError> {
    // Generate operation hash
    let operation_encoded = match entry.operation() {
        Some(operation) => match OperationEncoded::try_from(operation) {
            Ok(op) => Ok(op),
            // Handling JsValue errors coming from cddl crate when compiling for wasm target....
            Err(e) => Err(EntrySignedError::OperationEncodingError(
                e.as_string().unwrap(),
            )),
        }?,
        None => return Err(EntrySignedError::OperationMissing),
    };
    let operation_hash = operation_encoded.hash();
    let operation_size = operation_encoded.size();

    // Convert entry links to bamboo-rs `YasmfHash` type
    let backlink = entry.backlink_hash().map(|link| link.to_owned().into());
    let lipmaa_link = if entry.is_skiplink_required() {
        if entry.skiplink_hash().is_none() {
            return Err(EntrySignedError::SkiplinkMissing);
        }
        entry.skiplink_hash().map(|link| link.to_owned().into())
    } else {
        // Omit skiplink when it is the same as backlink, this saves us some bytes
        None
    };

    // Create Bamboo entry. See: https://github.com/AljoschaMeyer/bamboo#encoding for encoding
    // details and definition of entry fields.
    let mut entry: BambooEntry<_, &[u8]> = BambooEntry {
        log_id: entry.log_id().as_i64() as u64,
        is_end_of_feed: false,
        payload_hash: operation_hash.into(),
        payload_size: operation_size as u64,
        author: key_pair.public_key().to_owned(),
        seq_num: entry.seq_num().as_i64() as u64,
        backlink,
        lipmaa_link,
        sig: None,
    };

    let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];

    // Get unsigned entry bytes
    let entry_size = entry.encode(&mut entry_bytes)?;

    // Sign and add signature to entry
    let signature = key_pair.sign(&entry_bytes[..entry_size]);
    let signature_bytes = signature.to_bytes();
    entry.sig = Some(BambooSignature(&signature_bytes[..]));

    // Get signed entry bytes
    let signed_entry_size = entry.encode(&mut entry_bytes)?;

    // Return signed entry bytes in the form of an EntrySigned
    EntrySigned::try_from(&entry_bytes[..signed_entry_size])
}
