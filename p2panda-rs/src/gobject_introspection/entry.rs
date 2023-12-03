// SPDX-License-Identifier: AGPL-3.0-or-later

extern crate libc;

use libc::c_char;
use std::ffi::CStr;
use std::ffi::CString;

use crate::entry::traits::AsEntry;
use crate::entry::{EncodedEntry, LogId, SeqNum};
use crate::hash::Hash;
use crate::operation::EncodedOperation;

/// Return value of [`decode_entry`] that holds the decoded entry and plain operation.
#[repr(C)]
pub struct Entry {
    /// PublicKey of this entry.
    pub public_key: *mut c_char,

    /// Used log for this entry.
    pub log_id: u64,

    /// Sequence number of this entry.
    pub seq_num: u64,

    /// Hash of skiplink Bamboo entry.
    pub skiplink: *mut c_char,

    /// Hash of previous Bamboo entry.
    pub backlink: *mut c_char,

    /// Payload size of entry.
    pub payload_size: u64,

    /// Hash of payload.
    pub payload_hash: *mut c_char,

    /// Ed25519 signature of entry.
    pub signature: *mut c_char,
}

/// Decodes an hexadecimal string into an `Entry`.
#[no_mangle]
pub extern fn decode_entry(encoded_entry: *const c_char) -> *mut Entry {
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
