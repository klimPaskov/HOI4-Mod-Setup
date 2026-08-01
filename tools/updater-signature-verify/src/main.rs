use base64::{engine::general_purpose::STANDARD, Engine as _};
use minisign_verify::{PublicKey, Signature};
use serde_json::Value;
use std::env;
use std::fs::File;
use std::io::Read;

fn run() -> Result<(), String> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 3 {
        return Err("usage: updater-signature-verify <artifact> <signature> <tauri-config>".into());
    }
    let configuration_text = std::fs::read_to_string(&arguments[2])
        .map_err(|_| "updater configuration cannot be read")?;
    let configuration: Value = serde_json::from_str(&configuration_text)
        .map_err(|_| "updater configuration is invalid")?;
    let encoded_key = configuration
        .pointer("/plugins/updater/pubkey")
        .and_then(Value::as_str)
        .ok_or("updater public key is missing")?;
    let key_file = STANDARD
        .decode(encoded_key)
        .map_err(|_| "updater public key encoding is invalid")?;
    let key_file =
        std::str::from_utf8(&key_file).map_err(|_| "updater public key text is invalid")?;
    let public_key = PublicKey::decode(key_file).map_err(|_| "updater public key is invalid")?;
    let encoded_signature =
        std::fs::read_to_string(&arguments[1]).map_err(|_| "updater signature cannot be read")?;
    let signature_file = STANDARD
        .decode(encoded_signature.trim())
        .map_err(|_| "updater signature encoding is invalid")?;
    let signature_file =
        std::str::from_utf8(&signature_file).map_err(|_| "updater signature text is invalid")?;
    let signature =
        Signature::decode(signature_file).map_err(|_| "updater signature is invalid")?;
    let mut verifier = public_key
        .verify_stream(&signature)
        .map_err(|_| "updater signature cannot be verified")?;
    let mut artifact =
        File::open(&arguments[0]).map_err(|_| "updater artifact cannot be opened")?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = artifact
            .read(&mut buffer)
            .map_err(|_| "updater artifact cannot be read")?;
        if read == 0 {
            break;
        }
        verifier.update(&buffer[..read]);
    }
    verifier
        .finalize()
        .map_err(|_| "updater signature does not match the artifact".to_string())
}

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
    println!("Updater signature verified.");
}
