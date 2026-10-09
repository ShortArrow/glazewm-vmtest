//! Talks to a running guest daemon at the address in `VMTEST_GUEST_DAEMON`.

use std::time::Duration;

use vmtest_guest_client::GuestDaemonClient;
use vmtest_protocol::{Request, Response};

#[test]
#[ignore = "needs a running guest daemon; set VMTEST_GUEST_DAEMON"]
fn a_live_daemon_answers_ping_and_topology() {
    let address = std::env::var("VMTEST_GUEST_DAEMON").expect("VMTEST_GUEST_DAEMON is set");
    let mut client = GuestDaemonClient::connect(address.as_str(), Duration::from_secs(5)).unwrap();

    assert_eq!(client.request(&Request::Ping).unwrap(), Response::Pong);
    let Response::Topology { displays } = client.request(&Request::Topology).unwrap() else {
        panic!("expected a topology response");
    };
    assert_eq!(displays.iter().filter(|d| d.is_primary).count(), 1);
}
