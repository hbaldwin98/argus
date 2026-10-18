//! A connection's opening: what every client is sent on connecting, and
//! what greeting the daemon adds to it.

use crate::test_host::transcript::Transcript;
use crate::test_host::Host;

#[tokio::test]
async fn connecting_and_greeting() {
    let host = Host::start().await;
    let mut t = Transcript::open(&host).await;
    t.hello().await;
    pin!(t.finish());
    host.stop().await;
}
