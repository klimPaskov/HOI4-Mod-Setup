//! Documents the user adds while describing a mod. The page reads the file;
//! the core turns it into a redacted, bounded excerpt with a reserved
//! `@context/` evidence path, exactly as on the desktop.

use crate::{js, to_json};
use hoi4_setup_core::context::document_from_bytes;
use wasm_bindgen::prelude::*;

/// `number` keeps references unique within the page session.
#[wasm_bindgen(js_name = contextDocument)]
pub fn context_document(number: u32, name: &str, bytes: &[u8]) -> Result<String, JsError> {
    to_json(&document_from_bytes(u64::from(number), name, bytes).map_err(js)?)
}
