// SPDX-License-Identifier: AGPL-3.0-or-later

extern crate libc;

use libc::c_char;
use std::ffi::CStr;
use std::iter;

/// Returns hash of an hexadecimal encoded value.
#[no_mangle]
pub extern fn generate_hash(value: *const c_char) -> *mut c_char {
    let c_str = unsafe {
        assert!(!value.is_null());

        CStr::from_ptr(value)
    };

    // Convert hexadecimal string to bytes
    let bytes = hex::decode(c_str.to_str().unwrap()).unwrap();

    // Hash the value and return it as a string
    let hash = crate::hash::Hash::new_from_bytes(&bytes);
    let c_str = CString::new(hash.to_string()).unwrap();
    c_str.into_raw();
}
