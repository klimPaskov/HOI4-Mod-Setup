#![no_main]

use hoi4_mod_setup::security::normalize_relative_path;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(value) = std::str::from_utf8(bytes) {
        let _ = normalize_relative_path(value);
    }
});
