// SPDX-License-Identifier: AGPL-3.0-or-later

use bamboo_rs_core_ed25519_yasmf::entry::{is_lipmaa_required, MAX_ENTRY_SIZE};
use bamboo_rs_core_ed25519_yasmf::{Entry as BambooEntry, Signature as BambooSignature};

use crate::entry::error::EntrySignedError;
use crate::entry::{EncodedEntry, Entry, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::EncodedOperation;

/// Takes an [`Entry`] and a [`KeyPair`], returns signed and encoded entry bytes in form of an
/// [`EncodedEntry`] instance.
///
/// After conversion the result is ready to be sent to a p2panda node.
///
/// ## Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use p2panda_rs::entry::{sign_and_encode, Entry, EncodedEntry, LogId, SeqNum};
/// use p2panda_rs::identity::KeyPair;
/// use p2panda_rs::operation::{Operation, OperationFields, OperationValue};
/// use p2panda_rs::schema::SchemaId;
///
/// // Generate Ed25519 key pair to sign entry with
/// let key_pair = KeyPair::new();
///
/// // Create operation
/// let schema_id =
///     SchemaId::new("venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b")?;
/// let mut fields = OperationFields::new();
/// fields.add("title", OperationValue::Text("Hello, Panda!".to_owned()))?;
/// let operation = Operation::new_create(schema_id, fields)?;
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
pub fn sign_entry(
    backlink_hash: Option<&Hash>,
    skiplink_hash: Option<&Hash>,
    log_id: &LogId,
    seq_num: &SeqNum,
    payload: &EncodedOperation,
    key_pair: &KeyPair,
) -> Result<Entry, EntrySignedError> {
    // Generate payload hash and size from operation bytes
    let payload_hash = payload.hash();
    let payload_size = payload.size();

    // Convert entry links to bamboo-rs `YasmfHash` type
    let backlink = backlink_hash.map(|link| link.into());

    let lipmaa_link = if is_lipmaa_required(seq_num.as_u64()) {
        skiplink_hash.map(|link| link.into())
    } else {
        // Omit skiplink when it is the same as backlink, this saves us some bytes
        None
    };

    // Create Bamboo entry instance.
    //
    // See: https://github.com/AljoschaMeyer/bamboo#encoding for encoding details and definition of
    // entry fields.
    let mut entry: BambooEntry<_, &[u8]> = BambooEntry {
        is_end_of_feed: false,
        author: key_pair.public_key().to_owned(),
        log_id: log_id.as_u64(),
        seq_num: seq_num.as_u64(),
        lipmaa_link,
        backlink,
        payload_size,
        payload_hash: (&payload_hash).into(),
        sig: None,
    };

    let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];

    // Get unsigned entry bytes
    let entry_size = entry.encode(&mut entry_bytes)?;

    // Sign entry
    let signature = key_pair.sign(&entry_bytes[..entry_size]);
    let signature_bytes = signature.to_bytes();

    Ok(Entry {
        author: key_pair.public_key().into(),
        log_id: log_id.to_owned(),
        seq_num: seq_num.to_owned(),
        skiplink: skiplink_hash.cloned(),
        backlink: backlink_hash.cloned(),
        payload_size,
        payload_hash,
        signature: signature_bytes[..].into(),
    })
}

pub fn encode_entry(entry: &Entry) -> Result<EncodedEntry, EntrySignedError> {
    let signature_bytes = entry.signature().to_bytes();

    let mut entry: BambooEntry<_, &[u8]> = BambooEntry {
        is_end_of_feed: false,
        author: entry.author().into(),
        log_id: entry.log_id().as_u64(),
        seq_num: entry.seq_num().as_u64(),
        lipmaa_link: entry.skiplink().map(|link| link.into()),
        backlink: entry.backlink().map(|link| link.into()),
        payload_size: entry.payload_size(),
        payload_hash: entry.payload_hash().into(),
        sig: Some(BambooSignature(&signature_bytes[..])),
    };

    let mut entry_bytes = [0u8; MAX_ENTRY_SIZE];
    let signed_entry_size = entry.encode(&mut entry_bytes)?;

    Ok(EncodedEntry::from(&entry_bytes[..signed_entry_size]))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::{Entry, EntryBuilder, LogId, SeqNum};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::{OperationBuilder, OperationValue};
    use crate::schema::Schema;
    use crate::test_utils::fixtures::{key_pair, random_hash, schema};

    use super::{encode_entry, sign_entry};

    #[rstest]
    fn sign(
        #[from(random_hash)] skiplink_hash: Hash,
        #[from(random_hash)] backlink_hash: Hash,
        schema: Schema,
        key_pair: KeyPair,
    ) {
        let operation = OperationBuilder::new(&schema)
            .fields(&[("test", OperationValue::Text("test".to_owned()))])
            .build()
            .unwrap();

        let entry = EntryBuilder::new()
            .seq_num(&SeqNum::new(1).unwrap())
            .log_id(&LogId::new(0))
            .skiplink(&skiplink_hash)
            .backlink(&backlink_hash)
            .operation(&operation)
            .sign(&key_pair)
            .unwrap();
    }
}
