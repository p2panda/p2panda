use std::panic;

use console_error_panic_hook::hook as panic_hook;
use wasm_bindgen::prelude::wasm_bindgen;

/// Sets a [`panic hook`] for better error messages in NodeJS or web browser.
///
/// [`panic hook`]: https://crates.io/crates/console_error_panic_hook
#[wasm_bindgen(js_name = setWasmPanicHook)]
pub fn set_wasm_panic_hook() {
    panic::set_hook(Box::new(panic_hook));
}
