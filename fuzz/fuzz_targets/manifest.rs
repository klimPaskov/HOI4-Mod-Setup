#![no_main]

use hoi4_mod_setup::source::parse_manifest;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let _ = parse_manifest(bytes, None);
});
