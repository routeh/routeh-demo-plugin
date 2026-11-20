//! `settle_after`: an apply waits for it.
use crate::support::{Bench, settings, tunnel};
use routeh_plugin_sdk::protocol::channel;
use std::time::Duration;

#[test]
fn it_says_it_settled_only_once_settle_after_passed_since_the_desired_came() {
    let mut bench = Bench::new();
    let desired = channel::desired(&settings("settle_after = \"5s\""), &[]);
    assert!(!bench.converge(&desired).settled);
    bench.time.pass(Duration::from_secs(4));
    assert!(!bench.converge(&desired).settled);
    bench.time.pass(Duration::from_secs(1));
    assert!(bench.converge(&desired).settled);
    // A new desired waits again.
    let other = channel::desired(&settings("settle_after = \"5s\"\ngreeting = \"x\""), &[]);
    assert!(!bench.converge(&other).settled);
}

#[test]
fn a_tunnel_simulated_starting_never_settles() {
    let mut bench = Bench::new();
    let desired = channel::desired(
        &settings(""),
        &[tunnel("vpn", 256, "simulate = \"starting\"")],
    );
    bench.time.pass(Duration::from_secs(3600));
    assert!(!bench.converge(&desired).settled);
}
