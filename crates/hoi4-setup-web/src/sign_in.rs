//! OpenRouter sign-in in the browser. The page opens openrouter.ai in a
//! popup; OpenRouter returns a code to the callback page, which hands it back
//! over a same-origin channel. The PKCE verifier never leaves this tab, so the
//! code is useless to anyone else, and the key is kept in memory only.

use crate::js;
use hoi4_setup_core::free_assistant::{
    authorization_url, code_challenge, code_verifier, key_exchange_body, read_exchanged_key,
    sign_in_state,
};
use hoi4_setup_core::AppError;
use serde_json::json;
use wasm_bindgen::prelude::*;

/// Start a sign-in: `random` is at least 48 bytes from the Web Crypto API.
/// Returns `{ url, verifier, state }`.
#[wasm_bindgen(js_name = openRouterSignIn)]
pub fn open_router_sign_in(callback: &str, random: &[u8]) -> Result<String, JsError> {
    start(callback, random).map_err(js)
}

pub(crate) fn start(callback: &str, random: &[u8]) -> Result<String, AppError> {
    if random.len() < 48 {
        return Err(AppError::InvalidInput(
            "sign-in needs 48 random bytes".into(),
        ));
    }
    let verifier = code_verifier(&random[..32])?;
    let state = sign_in_state(&random[32..48])?;
    let url = authorization_url(callback, &code_challenge(&verifier), &state)?;
    Ok(json!({ "url": url, "verifier": verifier, "state": state }).to_string())
}

/// The key-exchange request body for a returned code.
#[wasm_bindgen(js_name = openRouterKeyRequest)]
pub fn open_router_key_request(code: &str, verifier: &str) -> Result<String, JsError> {
    key_exchange_body(code, verifier).map_err(js)
}

/// The user's key from OpenRouter's exchange response.
#[wasm_bindgen(js_name = openRouterKey)]
pub fn open_router_key(response: &str) -> Result<String, JsError> {
    read_exchanged_key(response).map_err(js)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_starts_with_a_fresh_verifier_and_state() {
        let random: Vec<u8> = (0..48).collect();
        let started: serde_json::Value = serde_json::from_str(
            &start(
                "https://klimpaskov.github.io/HOI4-Mod-Setup/app/openrouter-callback.html",
                &random,
            )
            .unwrap(),
        )
        .unwrap();
        let url = started["url"].as_str().unwrap();
        let verifier = started["verifier"].as_str().unwrap();
        assert!(url.starts_with("https://openrouter.ai/auth?"));
        assert!(url.contains(&code_challenge(verifier)));
        assert!(!url.contains(verifier));
        assert!(url.contains(started["state"].as_str().unwrap()));
        assert!(start("https://example.org/cb", &random[..40]).is_err());
        assert!(start("http://example.org/cb", &random).is_err());
    }
}
