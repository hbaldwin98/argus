//! The pane API's grammar, on any request line an agent's helper or a
//! stray local process sends to the loopback listener.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|line: &str| {
    let _ = argus_protocol::parse_pane_path(line);
    let _ = argus_protocol::parse_pane_url(line);
    let _ = argus_protocol::parse_request_target(line);
    let _ = argus_protocol::parse_extension_path(line);
});
