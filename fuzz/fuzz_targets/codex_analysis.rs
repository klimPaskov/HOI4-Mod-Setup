#![no_main]

use hoi4_mod_setup::codex::validate_analysis_payload;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let _ = validate_analysis_payload(bytes);
});
