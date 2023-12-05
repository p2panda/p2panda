// SPDX-License-Identifier: AGPL-3.0-or-later

use libc::c_char;
use std::ffi::CStr;
use std::ffi::CString;
use glib_sys::g_strdup;

/// p2panda_generate_hash:
///
/// Returns bub hash of an hexadecimal encoded value.
#[no_mangle]
pub extern fn p2panda_generate_hash(value: *const c_char) -> *mut c_char {
    let c_str = unsafe {
        assert!(!value.is_null());

        CStr::from_ptr(value)
    };

    // Convert hexadecimal string to bytes
    let bytes = hex::decode(c_str.to_str().unwrap()).unwrap();

    // Hash the value and return it as a string
    let hash = crate::hash::Hash::new_from_bytes(&bytes);
    let c_string = CString::new(hash.to_string()).unwrap();
    unsafe { g_strdup(c_string.as_ptr()) }
}
