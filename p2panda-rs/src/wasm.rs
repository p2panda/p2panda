use std::convert::TryFrom;
use std::panic;

use console_error_panic_hook::hook as panic_hook;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::atomic::MessageFields as PandaMessageFields;
use crate::atomic::{
    Entry, EntrySigned, Hash, LogId, Message, MessageEncoded, MessageValue, SeqNum,
};
use crate::key_pair::KeyPair;

macro_rules! jserr {
    ($l:expr) => {
        $l.map_err::<JsValue, _>(|err| js_sys::Error::new(&format!("{}", err)).into())?;
    };
}

/// Sets a [`panic hook`] for better error messages in NodeJS or web browser.
///
/// [`panic hook`]: https://crates.io/crates/console_error_panic_hook
#[wasm_bindgen(js_name = setWasmPanicHook)]
pub fn set_wasm_panic_hook() {
    panic::set_hook(Box::new(panic_hook));
}

#[wasm_bindgen]
pub struct MessageFields(PandaMessageFields);

#[wasm_bindgen]
impl MessageFields {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(PandaMessageFields::new())
    }

    pub fn add(&mut self, name: String, value: JsValue) -> Result<(), JsValue> {
        // @TODO: Add more types
        let field = match value.as_string() {
            Some(text) => Ok(MessageValue::Text(text)),
            None => Err(js_sys::Error::new(&format!("Invalid value type"))),
        }?;

        jserr!(self.0.add(&name, field));

        Ok(())
    }
}

#[wasm_bindgen(js_name = encodeCreateMessage)]
pub fn encode_create_message(schema: String, fields: MessageFields) -> Result<String, JsValue> {
    let hash = jserr!(Hash::new(&schema));
    let message = jserr!(Message::new_create(hash, fields.0));
    let message_encoded = jserr!(MessageEncoded::try_from(&message));
    Ok(message_encoded.as_str().to_owned())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignEntryResult {
    pub entry_encoded: String,
    pub entry_hash: String,
}

#[wasm_bindgen(js_name = signEntry)]
pub fn sign_entry(
    key_pair: KeyPair,
    encoded_message: String,
    entry_skiplink_hash: Option<String>,
    entry_backlink_hash: Option<String>,
    previous_seq_num: Option<i64>,
    log_id: i64,
) -> Result<JsValue, JsValue> {
    // If skiplink_hash exists construct Hash
    let skiplink_hash = match entry_skiplink_hash {
        Some(hash) => Some(jserr!(Hash::new(&hash))),
        None => None,
    };

    // If backlink_hash exists construct Hash
    let backlink_hash = match entry_backlink_hash {
        Some(hash) => Some(jserr!(Hash::new(&hash))),
        None => None,
    };

    // If seq_num exists construct SeqNum
    let seq_num = match previous_seq_num {
        Some(num) => Some(jserr!(SeqNum::new(num))),
        None => None,
    };

    // Convert to Message
    let message_encoded = jserr!(MessageEncoded::new(&encoded_message));
    let message = jserr!(Message::try_from(&message_encoded));

    // Create Entry instance
    let entry = jserr!(Entry::new(
        &LogId::new(log_id),
        &message,
        skiplink_hash.as_ref(),
        backlink_hash.as_ref(),
        seq_num.as_ref(),
    ));

    // Finally sign and encode entry
    let entry_signed = jserr!(EntrySigned::try_from((&entry, &key_pair)));

    // Serialize result to JSON
    let result = jserr!(wasm_bindgen::JsValue::from_serde(&SignEntryResult {
        entry_encoded: entry_signed.as_str().into(),
        entry_hash: entry_signed.hash().as_hex().into(),
    }));
    Ok(result)
}

#[wasm_bindgen(js_name = decodeEntry)]
pub fn decode_entry(
    entry_encoded: String,
    message_encoded: Option<String>,
) -> Result<JsValue, JsValue> {
    // Convert encoded message
    let message_encoded = match message_encoded {
        Some(msg) => {
            let inner = jserr!(MessageEncoded::new(&msg));
            Some(inner)
        }
        None => None,
    };

    // Convert encoded entry
    let entry_signed = jserr!(EntrySigned::new(&entry_encoded));
    let entry = jserr!(Entry::try_from((&entry_signed, message_encoded.as_ref())));

    // Serialize struct to JSON
    let result = jserr!(wasm_bindgen::JsValue::from_serde(&entry));
    Ok(result)
}
