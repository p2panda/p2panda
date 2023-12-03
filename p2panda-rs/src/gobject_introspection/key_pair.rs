// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    ffi::{CStr, CString},
    ptr::NonNull,
};

use libc::c_char;

use crate::{identity::KeyPair as KeyPairNonC, test_utils::fixtures::private_key};

#[repr(C)]
pub struct KeyPair(KeyPairNonC);

#[no_mangle]
pub extern "C" fn key_pair_from_private_key(private_key: *const c_char) -> KeyPair {
    let private_key = unsafe {
        assert!(!private_key.is_null());

        let c_repr = CStr::from_ptr(private_key);
        c_repr.to_str().expect("convert the private key from C")
    };

    let key_pair_inner = KeyPairNonC::from_private_key_str(private_key).expect("get a key pair");

    KeyPair(key_pair_inner)
}

#[no_mangle]
pub extern "C" fn key_pair_public_key(instance: &KeyPair) -> *const c_char {
    let key = instance.0.public_key().to_bytes();
    CString::new(key).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn key_pair_private_key(instance: &KeyPair) -> *const c_char {
    let key = instance.0.private_key().to_bytes();
    CString::new(key).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn sign(instance: &KeyPair, value: *mut c_char) -> *const c_char {
    let value = unsafe {
        assert!(!value.is_null());

        CStr::from_ptr(value)
    };

    let value = value.to_str().unwrap();

    let signature = instance.0.sign(value.as_bytes());
    CString::new(signature.to_bytes()).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn verify_signature(public_key: *const c_char, bytes: *const c_char, signature: *const c_char) {
    todo!()
}