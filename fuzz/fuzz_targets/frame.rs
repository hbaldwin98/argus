//! A frame body from a peer, read as either message: what a wrong, wedged
//! or newer peer sends must decode or fail, never panic.

#![no_main]

use argus_protocol::{ClientMsg, ServerMsg};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|body: &[u8]| {
    let _ = rmp_serde::from_slice::<ClientMsg>(body);
    let _ = rmp_serde::from_slice::<ServerMsg>(body);
});
