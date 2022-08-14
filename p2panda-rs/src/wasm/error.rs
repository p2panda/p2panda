// SPDX-License-Identifier: AGPL-3.0-or-later

//! Helpers for better debugging and handling of errors of the WebAssembly & JavaScript world.
use std::panic;

use console_error_panic_hook::hook as panic_hook;
use wasm_bindgen::prelude::wasm_bindgen;

// Converts any Rust error type into `js_sys::Error` while preserving its error message.
//
// This helps propagating errors similar like we do in Rust but in WebAssembly contexts. It is
// possible to optionally use a custom error message when required.
macro_rules! jserr {
    // Convert error to js_sys::Error with original error message
    ($l:expr) => {
        $l.map_err::<JsValue, _>(|err| js_sys::Error::new(&format!("{}", err)).into())?
    };

    // Convert error to js_sys::Error with custom error message
    ($l:expr, $err:expr) => {
        $l.map_err::<JsValue, _>(|_| js_sys::Error::new(&format!("{:?}", $err)).into())?
    };
}

/// Sets a [`panic hook`] for better error messages in NodeJS or web browser.
///
/// [`panic hook`]: https://crates.io/crates/console_error_panic_hook
#[wasm_bindgen(js_name = setWasmPanicHook)]
pub fn set_wasm_panic_hook() {
    panic::set_hook(Box::new(panic_hook));
}

pub(crate) use jserr;
