// SPDX-License-Identifier: AGPL-3.0-or-later

extern crate libc;

use libc::c_char;
use std::ffi::CStr;
use std::ffi::CString;

/// p2panda_string: (free-func p2panda_string_free)
///
/// String type used by p2panda.
#[repr(C)]
pub struct string(*mut c_char);

impl string {
    pub fn new(s: &str) -> Self {
        Self(CString::new(s).unwrap().into_raw())
    }

    /// p2panda_string_new: (constructor)
    /// @s: (transfer none): the content of the string
    ///
    /// Create a new p2panda string.
    ///
    /// Returns: (transfer full): the newly allocated string
    #[no_mangle]
    pub extern fn p2panda_string_new(s: *const c_char) -> Self {
        let c_str = unsafe {
            assert!(!s.is_null());

            CStr::from_ptr(s)
        };
        let c_string = CString::new(c_str.to_str().unwrap());

        Self(c_string.unwrap().into_raw())
    }
}

/// p2panda_string_free:
/// @s: (transfer full): the string to free
///
/// Frees the string.
#[no_mangle]
pub extern fn p2panda_string_free(s: string) {
    unsafe {
        if s.0.is_null() {
            return;
        }
        CString::from_raw(s.0)
    };
}
