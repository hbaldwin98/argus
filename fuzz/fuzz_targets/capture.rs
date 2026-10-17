//! A note an agent or a person writes as a feature: read whole or as rows,
//! then checked. Any note reads as something.

#![no_main]

use argus_protocol::Capture;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|note: &str| {
    let _ = Capture::parse(note).checked(true);
    let _ = Capture::parse_rows(note).checked(false);
});
