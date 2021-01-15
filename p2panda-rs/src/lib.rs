//! # p2panda-rs
#![warn(
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

mod atomic;
mod error;
mod keypair;

pub use crate::atomic::{Entry, EntryEncoded, Hash, LogId, Message, MessageEncoded, SeqNum};
pub use crate::error::Result;
pub use crate::keypair::KeyPair;

// This crate improves debugging by forwarding panic messages to console.error
use console_error_panic_hook::hook as panic_hook;
use std::panic;
use wasm_bindgen::prelude::wasm_bindgen;

/// Sets a panic hook for better error messages in NodeJS or web browser. See:
/// https://crates.io/crates/console_error_panic_hook
#[wasm_bindgen(js_name = setWasmPanicHook)]
pub fn set_wasm_panic_hook() {
    panic::set_hook(Box::new(panic_hook));
}
