extern crate wasm_bindgen;

use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub fn hello() -> String {
    "Hallo, hier ist alles schön".into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
