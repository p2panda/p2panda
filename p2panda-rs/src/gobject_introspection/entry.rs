// SPDX-License-Identifier: AGPL-3.0-or-later

extern crate libc;

use libc::c_char;
use std::ffi::CStr;
use std::ffi::CString;

use crate::entry::traits::AsEntry;
use crate::entry::{EncodedEntry, LogId, SeqNum};
use crate::hash::Hash;
use crate::operation::EncodedOperation;
use crate::gobject_introspection::key_pair::KeyPair;

/// Entrygfgfdsfsdfsfsfsfsf:
///
/// Return value of [`decode_entry`] that holds the decoded entry and plain operation.
#[repr(C)]
pub struct Entry {
    /// public_key:
    ///
    /// PublicKey of this entry
    pub public_key: *mut c_char,

    /// log_id:
    ///
    /// Used log for this entry.
    pub log_id: u64,

    /// seq_num:
    ///
    /// Sequence number of this entry.
    pub seq_num: u64,

    /// skiplink:
    ///
    /// Hash of skiplink Bamboo entry.
    pub skiplink: *mut c_char,

    /// backlink:
    ///
    /// Hash of previous Bamboo entry.
    pub backlink: *mut c_char,

    /// payload_size:
    ///
    /// Payload size of entry.
    pub payload_size: u64,

    /// payload_hash:
    ///
    /// Hash of payload.
    pub payload_hash: *mut c_char,

    /// signature:
    ///
    /// Ed25519 signature of entry.
    pub signature: *mut c_char,
}

/// sign_and_encode:
///
/// Returns a signed Bamboo entry.
#[no_mangle]
pub extern fn p2panda_sign_and_encode_entry(
    log_id: u64,
    seq_num: u64,
    skiplink_hash: *const c_char,
    backlink_hash: *const c_char,
    payload: *const c_char,
    key_pair: *mut KeyPair,
) -> *mut c_char {
    // If skiplink_hash exists construct `Hash`
    let skiplink = unsafe {
        match skiplink_hash.is_null() {
            true => None,
            false => Some(Hash::new(CStr::from_ptr(skiplink_hash).to_str().unwrap()).unwrap()),
        }
    };
    // If backlink_hash exists construct `Hash`
    let backlink = unsafe {
        match backlink_hash.is_null() {
            true => None,
            false => Some(Hash::new(CStr::from_ptr(backlink_hash).to_str().unwrap()).unwrap()),
        }
    };

    let c_payload = unsafe {
        assert!(!payload.is_null());

        CStr::from_ptr(payload)
    };

    assert!(!key_pair.is_null());

    // Convert `SeqNum` and `LogId`
    let log_id = LogId::new(log_id);
    let seq_num = SeqNum::new(seq_num).unwrap();

    // Convert to `EncodedOperation`
    let operation_bytes = hex::decode(c_payload.to_str().unwrap()).unwrap();
    let operation_encoded = EncodedOperation::from_bytes(&operation_bytes);

    // Sign and encode entry
    let entry_encoded = crate::entry::encode::sign_and_encode_entry(
        &log_id,
        &seq_num,
        skiplink.as_ref(),
        backlink.as_ref(),
        &operation_encoded,
        unsafe { key_pair.as_ref().unwrap().as_inner() },
    ).unwrap();

    // Return result as a hexadecimal string
    CString::new(entry_encoded.to_string()).unwrap().into_raw()
}

/// decode_entry:
///
/// Decodes an hexadecimal string into an `Entry`.
#[no_mangle]
pub extern fn p2panda_decode_entry(encoded_entry: *const c_char) -> *mut Entry {
    let c_str = unsafe {
        assert!(!encoded_entry.is_null());

        CStr::from_ptr(encoded_entry)
    };

    // Convert hexadecimal string to bytes
    let entry_bytes = hex::decode(c_str.to_str().unwrap()).unwrap();
    let entry_encoded = EncodedEntry::from_bytes(&entry_bytes);

    // Decode Bamboo entry
    let entry: crate::entry::Entry = crate::entry::decode::decode_entry(&entry_encoded).unwrap();

    // Serialise result to C struct
    let c_entry = Entry {
        public_key: CString::new(entry.public_key().to_string().as_str()).unwrap().into_raw(),
        seq_num: entry.seq_num().as_u64(),
        log_id: entry.log_id().as_u64(),
        skiplink: CString::new(entry.skiplink().map(|hash| hash.to_string()).unwrap().as_str()).unwrap().into_raw(),
        backlink: CString::new(entry.backlink().map(|hash| hash.to_string()).unwrap().as_str()).unwrap().into_raw(),
        payload_size: entry.payload_size(),
        payload_hash: CString::new(entry.payload_hash().to_string().as_str()).unwrap().into_raw(),
        signature: CString::new(entry.signature().to_string().as_str()).unwrap().into_raw(),
    };
    Box::into_raw(Box::new(c_entry))
}
