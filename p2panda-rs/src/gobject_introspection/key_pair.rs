// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::CStr;

use libc::c_char;

use crate::{identity::KeyPair as KeyPairNonC, test_utils::fixtures::private_key};

#[repr(C)]
pub struct KeyPair(KeyPairNonC);

impl KeyPair {
    pub extern "C" fn from_private_key(private_key: *const c_char) -> KeyPair {
        let private_key = unsafe {
            let c_repr = CStr::from_ptr(private_key);
            c_repr.to_str().expect("convert the private key from C")
        };

        let key_pair_inner = KeyPairNonC::from_private_key_str(private_key).expect("get a key pair");

        KeyPair(key_pair_inner)
    }
}