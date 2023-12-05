// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    convert::TryFrom,
    ffi::{CStr, CString},
};

use ed25519_dalek::Signature;
use glib_sys::g_strdup;
use libc::{c_char, c_int};

use crate::identity::{KeyPair as KeyPairNonC, PublicKey};

/// p2panda_KeyPair: (free-func p2panda_key_pair_free)
pub struct KeyPair(KeyPairNonC);

/// p2panda_key_pair_new_from_private_key:
/// @private_key: (transfer none): the private key string
///
/// Returns: (transfer full): the created KeyPair
#[no_mangle]
pub extern "C" fn p2panda_key_pair_new_from_private_key(
    private_key: *const c_char,
) -> *mut KeyPair {
    let private_key = unsafe {
        assert!(!private_key.is_null());

        let c_repr = CStr::from_ptr(private_key);
        c_repr.to_str().expect("convert the private key from C")
    };

    let key_pair_inner = KeyPairNonC::from_private_key_str(private_key).expect("get a key pair");

    Box::into_raw(Box::new(KeyPair(key_pair_inner)))
}

#[no_mangle]
pub extern "C" fn p2panda_key_pair_free(instance: *mut KeyPair) {
    if instance.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(instance));
    }
}

impl KeyPair {
    /// Internal method to access non-wasm instance of `KeyPair`.
    pub(super) fn as_inner(&self) -> &KeyPairNonC {
        &self.0
    }
}

#[no_mangle]
pub extern "C" fn p2panda_key_pair_get_public_key(instance: *mut KeyPair) -> *mut c_char {
    let key_pair = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    let key = key_pair.0.public_key().to_bytes();
    let c_string = CString::new(key).unwrap();
    unsafe { g_strdup(c_string.as_ptr()) }
}

#[no_mangle]
pub extern "C" fn p2panda_key_pair_get_private_key(instance: *mut KeyPair) -> *mut c_char {
    let key_pair = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    let key = key_pair.0.private_key().to_bytes();
    let c_string = CString::new(key).unwrap();
    unsafe { g_strdup(c_string.as_ptr()) }
}

#[no_mangle]
pub extern "C" fn p2panda_key_pair_sign(
    instance: *mut KeyPair,
    value: *const c_char,
) -> *mut c_char {
    let key_pair = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    let c_str = unsafe {
        assert!(!value.is_null());

        CStr::from_ptr(value)
    };

    let signature = key_pair.0.sign(c_str.to_str().unwrap().as_bytes());
    let c_string = CString::new(signature.to_bytes()).unwrap();
    unsafe { g_strdup(c_string.as_ptr()) }
}

#[no_mangle]
pub extern "C" fn p2panda_key_pair_verify_signature(
    public_key: *const c_char,
    bytes: *const c_char,
    signature: *const c_char,
) -> c_int {
    let public_key = unsafe {
        assert!(!public_key.is_null());

        CStr::from_ptr(public_key)
    };

    let bytes = unsafe {
        assert!(!bytes.is_null());

        CStr::from_ptr(bytes)
    };

    let signature = unsafe {
        assert!(!signature.is_null());

        CStr::from_ptr(signature)
    };

    let public_key = PublicKey::new(public_key.to_str().unwrap()).unwrap();
    let signature = Signature::try_from(signature.to_bytes()).unwrap();
    match KeyPairNonC::verify(&public_key, bytes.to_bytes(), &signature) {
        Ok(_) => 1,
        Err(_) => 0,
    }
}
