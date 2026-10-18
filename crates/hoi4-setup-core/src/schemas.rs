//! Compile-once access to the checked-in JSON Schemas.
//!
//! Compiling a Draft 2020-12 validator is far more expensive than validating
//! one document, and the same few schemas are used on every manifest parse,
//! analysis turn, and generated JSON file. Each schema text is compiled the
//! first time it is used and reused for the life of the process.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The compiled validator for a checked-in schema (`include_str!` text).
/// The schema set is small and fixed, so compiled validators live for the
/// whole process.
pub fn validator(schema_text: &'static str) -> Result<&'static jsonschema::Validator, String> {
    static CACHE: OnceLock<Mutex<HashMap<usize, &'static jsonschema::Validator>>> = OnceLock::new();
    // `include_str!` text has a stable address, which identifies the schema
    // without hashing its contents on every call.
    let key = schema_text.as_ptr() as usize;
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(validator) = cache
        .lock()
        .map_err(|_| "schema cache is unavailable")?
        .get(&key)
    {
        return Ok(*validator);
    }
    let value: serde_json::Value = serde_json::from_str(schema_text)
        .map_err(|error| format!("checked-in JSON Schema is invalid JSON: {error}"))?;
    let compiled = jsonschema::draft202012::new(&value)
        .map_err(|error| format!("checked-in JSON Schema cannot be compiled: {error}"))?;
    let mut cache = cache.lock().map_err(|_| "schema cache is unavailable")?;
    Ok(*cache
        .entry(key)
        .or_insert_with(|| Box::leak(Box::new(compiled))))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = r#"{"type":"object","required":["a"]}"#;

    #[test]
    fn a_schema_is_compiled_once_and_reused() {
        let first = validator(SCHEMA).unwrap();
        let second = validator(SCHEMA).unwrap();
        assert!(std::ptr::eq(first, second));
        assert!(first.is_valid(&serde_json::json!({"a": 1})));
        assert!(!first.is_valid(&serde_json::json!({})));
        assert!(validator("not json").is_err());
    }
}
