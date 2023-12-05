// SPDX-License-Identifier: AGPL-3.0-or-later

use glib_sys::g_strdup;
use libc::c_char;
use std::ffi::CStr;
use std::ffi::CString;

/// p2panda_generate_hash:
/// @value: (transfer none): the string to hash
///
/// Returns hash of an hexadecimal encoded value.
///
/// Returns: (transfer full): the generated hash string
#[no_mangle]
pub extern "C" fn p2panda_generate_hash(value: *const c_char) -> *mut c_char {
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
