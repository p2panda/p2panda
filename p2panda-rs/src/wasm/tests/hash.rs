// SPDX-License-Identifier: AGPL-3.0-or-later

use wasm_bindgen_test::*;

use crate::wasm::generate_hash;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn generates_hashes() {
    let result = generate_hash("002211ff").unwrap();
    assert_eq!(
        result,
        "0020e9c4721f01121de2cbd66ca1a1a01607ee81c6200541b879eafc0be088808ed7"
    );
}

#[wasm_bindgen_test]
fn invalid_hex() {
    assert!(generate_hash("xhjk").is_err());
}
