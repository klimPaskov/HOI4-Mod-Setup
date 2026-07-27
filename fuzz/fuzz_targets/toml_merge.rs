#![no_main]

use hoi4_mod_setup::merge::structured_toml_merge;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(text) = std::str::from_utf8(bytes) {
        let _ = structured_toml_merge(text, text, text);
    }
});
