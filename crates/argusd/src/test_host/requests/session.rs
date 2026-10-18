//! The greeting a client opens with, any number of times: this build's,
//! one without some of its capabilities, one from a build that speaks
//! another protocol, or one listing capabilities nobody has.

use argus_protocol::{ClientMsg, Hello, CAPABILITIES};
use proptest::prelude::*;

use super::values::text;

pub fn hello() -> BoxedStrategy<ClientMsg> {
    let ours: Vec<String> = CAPABILITIES.iter().map(|c| c.to_string()).collect();
    let capabilities = (proptest::sample::subsequence(ours.clone(), 0..=ours.len()), proptest::collection::vec(text(), 0..2))
        .prop_map(|(mut listed, strange)| {
            listed.extend(strange);
            listed
        });
    let protocol = prop_oneof![4 => Just(Hello::this_build().protocol), 1 => any::<u32>()];
    let version = prop_oneof![3 => Just(Hello::this_build().version), 1 => text()];
    prop_oneof![
        2 => Just(Hello::this_build()),
        3 => (protocol, version, capabilities).prop_map(|(protocol, version, capabilities)| Hello { protocol, version, capabilities }),
    ]
    .prop_map(ClientMsg::Hello)
    .boxed()
}
