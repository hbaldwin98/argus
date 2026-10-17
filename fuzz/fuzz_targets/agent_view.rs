//! A view an agent publishes, as the JSON it sends. Whatever it sends is
//! refused or checked without panicking, and a view that passed is one
//! the client, reading it back off the wire, passes again unchanged.

#![no_main]

use argus_protocol::AgentView;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|json: &[u8]| {
    let Ok(view) = serde_json::from_slice::<AgentView>(json) else {
        return;
    };
    let Ok(view) = view.checked() else {
        return;
    };
    let sent = serde_json::to_vec(&view).unwrap();
    let read: AgentView = serde_json::from_slice(&sent).unwrap();
    assert_eq!(read.clone().checked(), Ok(view));
});
