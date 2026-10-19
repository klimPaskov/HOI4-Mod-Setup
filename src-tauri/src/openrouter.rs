//! OpenRouter sign-in for the desktop app.
//!
//! OpenRouter offers third-party apps an OAuth PKCE flow: the user approves
//! on openrouter.ai in their own browser, OpenRouter redirects to a one-time
//! loopback address with a code, and the app exchanges the code for a key
//! tied to the user's own account. The key goes straight to the OS vault as
//! the OpenRouter provider credential, exactly like a pasted key. The app
//! never sees the user's OpenRouter password or session.

use crate::AppError;
use hoi4_setup_core::free_assistant::{
    authorization_url, code_challenge, code_verifier, key_exchange_body, read_exchanged_key,
    OPENROUTER_KEY_EXCHANGE_URL,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long the loopback address waits for the browser.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_EXCHANGE_BYTES: u64 = 64 * 1024;

/// A sign-in in progress: the page to open and the loopback listener that
/// receives the code.
pub struct PendingSignIn {
    pub url: String,
    listener: TcpListener,
    verifier: String,
    state: String,
}

impl PendingSignIn {
    pub fn start() -> Result<Self, AppError> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|_| AppError::Process("sign-in could not open a local address".into()))?;
        let port = listener
            .local_addr()
            .map_err(|_| AppError::Process("sign-in could not open a local address".into()))?
            .port();
        let mut random = Vec::with_capacity(48);
        for _ in 0..3 {
            random.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        }
        let verifier = code_verifier(&random)?;
        let state = uuid::Uuid::new_v4().simple().to_string();
        let url = authorization_url(
            &format!("http://127.0.0.1:{port}/callback"),
            &code_challenge(&verifier),
            &state,
        )?;
        Ok(Self {
            url,
            listener,
            verifier,
            state,
        })
    }

    /// Wait for OpenRouter's redirect, then exchange the code for the key.
    pub fn finish(self, cancelled: &Arc<AtomicBool>) -> Result<String, AppError> {
        let code = wait_for_code(&self.listener, &self.state, SIGN_IN_TIMEOUT, cancelled)?;
        exchange(&code, &self.verifier)
    }
}

/// Accept loopback requests until one carries the code for this sign-in.
pub(crate) fn wait_for_code(
    listener: &TcpListener,
    state: &str,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<String, AppError> {
    listener
        .set_nonblocking(true)
        .map_err(|_| AppError::Process("sign-in could not wait for the browser".into()))?;
    let deadline = Instant::now() + timeout;
    loop {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::Process("sign-in was cancelled".into()));
        }
        if Instant::now() >= deadline {
            return Err(AppError::Process(
                "sign-in timed out; choose Sign in with OpenRouter again".into(),
            ));
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(outcome) = answer(stream, state) {
                    return outcome;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => {
                return Err(AppError::Process(
                    "sign-in could not wait for the browser".into(),
                ))
            }
        }
    }
}

/// Read one request and answer it. `None` means the request was not the
/// callback (a favicon fetch, for example) and waiting continues.
fn answer(mut stream: TcpStream, state: &str) -> Option<Result<String, AppError>> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = vec![0; MAX_REQUEST_BYTES];
    let mut read = 0;
    while read < buffer.len() {
        match stream.read(&mut buffer[read..]) {
            Ok(0) => break,
            Ok(count) => {
                read += count;
                if buffer[..read]
                    .windows(4)
                    .any(|window| window == b"\r\n\r\n")
                {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let request = String::from_utf8_lossy(&buffer[..read]);
    let line = request.lines().next().unwrap_or_default();
    let outcome = parse_callback(line, state);
    let (status, message) = match &outcome {
        None => ("404 Not Found", "Not found."),
        Some(Ok(_)) => (
            "200 OK",
            "Signed in to OpenRouter. You can close this tab and return to HOI4 Mod Setup.",
        ),
        Some(Err(_)) => (
            "400 Bad Request",
            "OpenRouter sign-in did not finish. Return to HOI4 Mod Setup and try again.",
        ),
    };
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>HOI4 Mod Setup</title><p>{message}</p>"
    );
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status}\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\ncache-control: no-store\r\nreferrer-policy: no-referrer\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    );
    outcome
}

/// The code from a callback request line, checked against this sign-in's
/// state. `None` when the request is for another path.
pub(crate) fn parse_callback(line: &str, state: &str) -> Option<Result<String, AppError>> {
    let mut parts = line.split(' ');
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        return None;
    };
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != "/callback" {
        return None;
    }
    let pairs = url.query_pairs().into_owned().collect::<Vec<_>>();
    let value = |name: &str| {
        pairs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    // OpenRouter echoes `state` on success; a different value is another
    // sign-in or a forged request.
    if value("state").as_deref() != Some(state) {
        return Some(Err(AppError::Credential(
            "the sign-in response did not match this sign-in".into(),
        )));
    }
    match value("code") {
        Some(code) if !code.is_empty() => Some(Ok(code)),
        _ => Some(Err(AppError::Credential(
            "OpenRouter sign-in was not approved".into(),
        ))),
    }
}

fn exchange(code: &str, verifier: &str) -> Result<String, AppError> {
    let body = key_exchange_body(code, verifier)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| AppError::Process("sign-in could not reach OpenRouter".into()))?;
    let response = client
        .post(OPENROUTER_KEY_EXCHANGE_URL)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .body(body)
        .send()
        .map_err(|_| AppError::Process("sign-in could not reach OpenRouter".into()))?;
    let status = response.status();
    let mut text = String::new();
    response
        .take(MAX_EXCHANGE_BYTES)
        .read_to_string(&mut text)
        .map_err(|_| AppError::Process("OpenRouter's sign-in response was unreadable".into()))?;
    if !status.is_success() {
        return Err(AppError::Credential(format!(
            "OpenRouter did not accept the sign-in (HTTP {})",
            status.as_u16()
        )));
    }
    read_exchanged_key(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sign_in_page_names_only_this_loopback_callback() {
        let pending = PendingSignIn::start().unwrap();
        let url = reqwest::Url::parse(&pending.url).unwrap();
        assert_eq!(url.origin().ascii_serialization(), "https://openrouter.ai");
        let port = pending.listener.local_addr().unwrap().port();
        let pairs = url.query_pairs().into_owned().collect::<Vec<_>>();
        assert!(pairs.contains(&(
            "callback_url".into(),
            format!("http://127.0.0.1:{port}/callback")
        )));
        assert!(pairs.contains(&("code_challenge_method".into(), "S256".into())));
        assert!(pairs.contains(&("state".into(), pending.state.clone())));
        assert!(!pending.url.contains(&pending.verifier));
    }

    #[test]
    fn callbacks_must_match_the_state() {
        assert!(parse_callback("GET /favicon.ico HTTP/1.1", "s1").is_none());
        assert!(parse_callback("POST /callback?code=a&state=s1 HTTP/1.1", "s1").is_none());
        assert_eq!(
            parse_callback("GET /callback?code=abc&state=s1 HTTP/1.1", "s1")
                .unwrap()
                .unwrap(),
            "abc"
        );
        assert!(
            parse_callback("GET /callback?code=abc&state=other HTTP/1.1", "s1")
                .unwrap()
                .is_err()
        );
        assert!(parse_callback("GET /callback?code=abc HTTP/1.1", "s1")
            .unwrap()
            .is_err());
        assert!(parse_callback("GET /callback?state=s1 HTTP/1.1", "s1")
            .unwrap()
            .is_err());
    }

    #[test]
    fn the_loopback_listener_ignores_other_paths_and_answers_the_browser() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let browser = std::thread::spawn(move || {
            let mut replies = Vec::new();
            for target in ["/favicon.ico", "/callback?code=abc&state=s1"] {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                stream
                    .write_all(
                        format!("GET {target} HTTP/1.1\r\nhost: 127.0.0.1\r\n\r\n").as_bytes(),
                    )
                    .unwrap();
                let mut reply = String::new();
                stream.read_to_string(&mut reply).unwrap();
                replies.push(reply);
            }
            replies
        });
        let cancelled = AtomicBool::new(false);
        let code = wait_for_code(&listener, "s1", Duration::from_secs(20), &cancelled).unwrap();
        assert_eq!(code, "abc");
        let replies = browser.join().unwrap();
        assert!(replies[0].starts_with("HTTP/1.1 404"));
        assert!(replies[1].starts_with("HTTP/1.1 200"));
        assert!(!replies[1].contains("abc"));
    }

    #[test]
    fn waiting_stops_on_cancel_and_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let cancelled = AtomicBool::new(true);
        assert!(
            wait_for_code(&listener, "s1", Duration::from_secs(20), &cancelled)
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
        let cancelled = AtomicBool::new(false);
        assert!(
            wait_for_code(&listener, "s1", Duration::from_millis(200), &cancelled)
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
    }
}
