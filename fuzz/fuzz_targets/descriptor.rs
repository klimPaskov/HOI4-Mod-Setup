#![no_main]

use hoi4_mod_setup::descriptors::{parse_descriptor, validate_thumbnail_png};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let _ = parse_descriptor(bytes);
    let _ = validate_thumbnail_png(bytes);
});
