//! The free setup assistant and OpenRouter sign-in.
//!
//! The free route needs no account: a relay run by the project's maintainer
//! (`relay/`) forwards one structured request to a free OpenRouter model with
//! the maintainer's key, which lives only in the relay. The checked-in
//! `src/free-assistant.json` names the relay; until one is deployed its
//! endpoint is `null` and the route is not offered.
//!
//! OpenRouter sign-in is the OAuth PKCE flow OpenRouter offers to third-party
//! apps: the user approves on openrouter.ai and the app exchanges the returned
//! code for a key tied to the user's own account. Hosts supply the randomness
//! and the HTTP calls.

use crate::AppError;
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const FREE_PROVIDER_ID: &str = "free";
/// The authentication label recorded for the free route: no user credential.
pub const FREE_AUTH_MODE: &str = "shared_free";
pub const OPENROUTER_PROVIDER_ID: &str = "openrouter";
pub const OPENROUTER_ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";
pub const OPENROUTER_AUTH_URL: &str = "https://openrouter.ai/auth";
pub const OPENROUTER_KEY_EXCHANGE_URL: &str = "https://openrouter.ai/api/v1/auth/keys";
/// The free model suggested for a signed-in OpenRouter account.
pub const OPENROUTER_DEFAULT_MODEL: &str = "nvidia/nemotron-3.5-super-vl:free";

const CONFIG: &str = include_str!("../../../src/free-assistant.json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreeAssistant {
    pub endpoint: String,
    pub model: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    endpoint: Option<String>,
    model: String,
}

/// The deployed relay, or `None` while the checked-in endpoint is unset.
pub fn free_assistant() -> Option<FreeAssistant> {
    parse_config(CONFIG).ok().flatten()
}

fn parse_config(text: &str) -> Result<Option<FreeAssistant>, AppError> {
    let config: Config = serde_json::from_str(text)
        .map_err(|_| AppError::Serialization("the free assistant config is malformed".into()))?;
    if !is_model_id(&config.model) {
        return Err(AppError::Serialization(
            "the free assistant model ID is invalid".into(),
        ));
    }
    let Some(endpoint) = config.endpoint else {
        return Ok(None);
    };
    if !is_relay_endpoint(&endpoint) {
        return Err(AppError::Serialization(
            "the free assistant endpoint must be a plain HTTPS chat completions address".into(),
        ));
    }
    Ok(Some(FreeAssistant {
        endpoint,
        model: config.model,
    }))
}

fn is_model_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '-'))
}

fn is_relay_endpoint(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.host_str().is_some_and(|host| !host.is_empty())
            && url.path().ends_with("/chat/completions")
            && url.as_str() == value
    })
}

/// Whether a provider's request carries the output schema as an OpenAI-style
/// `response_format`, which these routes honour natively.
pub fn uses_response_format(provider: &str) -> bool {
    matches!(
        provider,
        "gemini" | OPENROUTER_PROVIDER_ID | FREE_PROVIDER_ID
    )
}

/// The PKCE S256 challenge for a verifier.
pub fn code_challenge(verifier: &str) -> String {
    base64_url(&Sha256::digest(verifier.as_bytes()))
}

/// A PKCE verifier from at least 32 random bytes the host supplies.
pub fn code_verifier(random: &[u8]) -> Result<String, AppError> {
    if random.len() < 32 {
        return Err(AppError::InvalidInput(
            "sign-in needs at least 32 random bytes".into(),
        ));
    }
    Ok(base64_url(random))
}

/// An unguessable `state` value from at least 16 random bytes.
pub fn sign_in_state(random: &[u8]) -> Result<String, AppError> {
    if random.len() < 16 {
        return Err(AppError::InvalidInput(
            "sign-in needs at least 16 random bytes".into(),
        ));
    }
    Ok(base64_url(random))
}

/// The openrouter.ai page where the user approves a key for this app.
/// `callback` is a loopback address on the desktop or the web edition's
/// callback page.
pub fn authorization_url(callback: &str, challenge: &str, state: &str) -> Result<String, AppError> {
    let callback_url = url::Url::parse(callback)
        .map_err(|_| AppError::InvalidInput("the sign-in callback is not a URL".into()))?;
    let loopback = callback_url.scheme() == "http"
        && matches!(callback_url.host_str(), Some("127.0.0.1" | "localhost"));
    if !(loopback || callback_url.scheme() == "https")
        || callback_url.query().is_some()
        || callback_url.fragment().is_some()
    {
        return Err(AppError::InvalidInput(
            "the sign-in callback must be HTTPS or this computer".into(),
        ));
    }
    if challenge.len() != 43
        || !is_base64_url(challenge)
        || !is_base64_url(state)
        || state.len() < 16
    {
        return Err(AppError::InvalidInput(
            "the sign-in challenge is invalid".into(),
        ));
    }
    let mut url = url::Url::parse(OPENROUTER_AUTH_URL).expect("the reviewed address parses");
    url.query_pairs_mut()
        .append_pair("callback_url", callback_url.as_str())
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);
    Ok(url.into())
}

/// The JSON body that exchanges the returned code for the user's key.
pub fn key_exchange_body(code: &str, verifier: &str) -> Result<String, AppError> {
    if code.is_empty()
        || code.len() > 512
        || code.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(AppError::InvalidInput(
            "OpenRouter returned an invalid sign-in code".into(),
        ));
    }
    Ok(serde_json::json!({
        "code": code,
        "code_verifier": verifier,
        "code_challenge_method": "S256",
    })
    .to_string())
}

/// The key in OpenRouter's exchange response.
pub fn read_exchanged_key(response: &str) -> Result<String, AppError> {
    let value: serde_json::Value = serde_json::from_str(response).map_err(|_| {
        AppError::Protocol("OpenRouter returned an unreadable sign-in response".into())
    })?;
    let key = value
        .get("key")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !key.starts_with("sk-or-")
        || key.len() > 256
        || key.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(AppError::Credential(
            "OpenRouter did not return a key".into(),
        ));
    }
    Ok(key.to_owned())
}

fn is_base64_url(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn base64_url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checked_in_config_parses() {
        let parsed = parse_config(CONFIG).unwrap();
        if let Some(free) = parsed {
            assert!(is_relay_endpoint(&free.endpoint));
            assert!(is_model_id(&free.model));
        }
    }

    #[test]
    fn relay_endpoints_must_be_plain_https_chat_routes() {
        let ok = r#"{"endpoint":"https://relay.example.workers.dev/v1/chat/completions","model":"a/b:free"}"#;
        assert_eq!(
            parse_config(ok).unwrap().unwrap().endpoint,
            "https://relay.example.workers.dev/v1/chat/completions"
        );
        assert!(parse_config(r#"{"endpoint":null,"model":"a/b:free"}"#)
            .unwrap()
            .is_none());
        for endpoint in [
            "http://relay.example/v1/chat/completions",
            "https://user:pw@relay.example/v1/chat/completions",
            "https://relay.example/v1/chat/completions?key=1",
            "https://relay.example:8443/v1/chat/completions",
            "https://relay.example/v1/models",
        ] {
            let text = format!(r#"{{"endpoint":"{endpoint}","model":"a/b:free"}}"#);
            assert!(parse_config(&text).is_err(), "{endpoint}");
        }
        assert!(parse_config(r#"{"endpoint":null,"model":"has space"}"#).is_err());
        assert!(parse_config(r#"{"endpoint":null,"model":"a","key":"x"}"#).is_err());
    }

    #[test]
    fn pkce_matches_the_rfc_7636_example() {
        // RFC 7636 appendix B.
        assert_eq!(
            code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(base64_url(b"f"), "Zg");
        assert_eq!(base64_url(b"fo"), "Zm8");
        assert_eq!(base64_url(b"foo"), "Zm9v");
        assert!(code_verifier(&[7; 31]).is_err());
        assert_eq!(code_verifier(&[0; 32]).unwrap().len(), 43);
    }

    #[test]
    fn the_authorization_url_carries_only_the_reviewed_parameters() {
        let challenge = code_challenge("verifier-verifier-verifier-verifier-verifier");
        let url = authorization_url(
            "http://127.0.0.1:53111/callback",
            &challenge,
            "abcdefghijklmnop",
        )
        .unwrap();
        let parsed = url::Url::parse(&url).unwrap();
        assert_eq!(
            parsed.origin().ascii_serialization(),
            "https://openrouter.ai"
        );
        let pairs: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
        assert_eq!(
            pairs,
            vec![
                (
                    "callback_url".into(),
                    "http://127.0.0.1:53111/callback".into()
                ),
                ("code_challenge".into(), challenge.clone()),
                ("code_challenge_method".into(), "S256".into()),
                ("state".into(), "abcdefghijklmnop".into()),
            ]
        );
        assert!(authorization_url(
            "https://klimpaskov.github.io/HOI4-Mod-Setup/app/openrouter-callback.html",
            &challenge,
            "abcdefghijklmnop"
        )
        .is_ok());
        assert!(authorization_url(
            "http://evil.example/callback",
            &challenge,
            "abcdefghijklmnop"
        )
        .is_err());
        assert!(authorization_url(
            "https://app.example/cb?next=1",
            &challenge,
            "abcdefghijklmnop"
        )
        .is_err());
        assert!(authorization_url("http://127.0.0.1:1/cb", "short", "abcdefghijklmnop").is_err());
        assert!(authorization_url("http://127.0.0.1:1/cb", &challenge, "short").is_err());
    }

    #[test]
    fn key_exchange_reads_only_an_openrouter_key() {
        let body: serde_json::Value =
            serde_json::from_str(&key_exchange_body("abc", "ver").unwrap()).unwrap();
        assert_eq!(body["code_challenge_method"], "S256");
        assert!(key_exchange_body("a b", "ver").is_err());
        assert!(key_exchange_body("", "ver").is_err());
        let key = format!("sk-or-v1-{}", "0".repeat(16));
        assert_eq!(
            read_exchanged_key(&format!(r#"{{"key":"{key}"}}"#)).unwrap(),
            key
        );
        assert!(read_exchanged_key(r#"{"key":"sk-other"}"#).is_err());
        assert!(read_exchanged_key(r#"{"error":"denied"}"#).is_err());
        assert!(read_exchanged_key("<html>").is_err());
    }
}
