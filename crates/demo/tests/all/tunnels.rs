//! A tunnel of the demo's kind: its device, its outer traffic, its phase.
use crate::support::{Bench, Kernel, settings, tunnel, value};
use demo::{Flow, FlowStats, KEEPALIVE};
use routeh_plugin_sdk::Plugin;
use routeh_plugin_sdk::protocol::channel::{self, Phase, Reading};
use std::time::Duration;

#[test]
fn each_tunnel_gets_a_device_of_kind_demo_and_is_up_without_an_endpoint() {
    let mut bench = Bench::new();
    let desired = channel::desired(&settings(""), &[tunnel("vpn", 256, "")]);
    let state = bench.converge(&desired);
    assert!(state.settled);
    let vpn = &state.instances["vpn"];
    assert_eq!(
        (vpn.phase, vpn.message.as_str(), vpn.peer.as_deref()),
        (Phase::Up, "", None)
    );
    assert_eq!(value(&vpn.values, "sent"), Some(&Reading::Count(0)));
    assert_eq!(value(&vpn.values, "keepalive"), None);
    assert_eq!(*bench.kernel.asked.lock().unwrap(), ["add tn_vpn demo"]);
    // No flow without an endpoint or a port.
    assert!(bench.flows.asked.lock().unwrap().is_empty());
    assert!(bench.lines().contains(&"demo/vpn: tn_vpn is up".to_owned()));
    // Converging again logs nothing new for it.
    let before = bench.lines().len();
    bench.converge(&desired);
    assert_eq!(bench.lines().len(), before);
}

#[test]
fn with_an_endpoint_it_pings_with_the_tunnels_mark_and_is_up_while_answered() {
    let mut bench = Bench::new();
    let desired = channel::desired(
        &settings(""),
        &[tunnel(
            "vpn",
            257,
            "endpoint = \"192.0.2.9:4000\"\nkeepalive = \"2s\"\nlisten_port = 4001",
        )],
    );
    let state = bench.converge(&desired);
    let flow = Flow {
        endpoint: Some("192.0.2.9:4000".into()),
        mark: 257,
        every: Duration::from_secs(2),
        listen: Some(4001),
    };
    assert_eq!(bench.flows.kept.lock().unwrap()["vpn"], flow);
    let vpn = &state.instances["vpn"];
    assert_eq!(
        (vpn.phase, vpn.message.as_str()),
        (Phase::Connecting, "no answer from 192.0.2.9:4000 yet")
    );
    assert_eq!(vpn.peer.as_deref(), Some("192.0.2.9:4000"));
    assert_eq!(value(&vpn.values, "keepalive"), Some(&Reading::Duration(2)));
    // Connecting is settled: it is out of starting.
    assert!(state.settled);

    let answered = FlowStats {
        sent: 3,
        sent_bytes: 48,
        answered: 2,
        heard: 1,
        last_answer: Some(bench.time.unix()),
        endpoint_address: Some("192.0.2.9".parse().unwrap()),
    };
    bench
        .flows
        .stats
        .lock()
        .unwrap()
        .insert("vpn".into(), answered);
    let state = bench.converge(&desired);
    let vpn = &state.instances["vpn"];
    assert_eq!(vpn.phase, Phase::Up);
    assert_eq!(value(&vpn.values, "sent"), Some(&Reading::Count(3)));
    assert_eq!(value(&vpn.values, "answered"), Some(&Reading::Count(2)));
    assert_eq!(value(&vpn.values, "heard"), Some(&Reading::Count(1)));
    assert_eq!(value(&vpn.values, "sent_bytes"), Some(&Reading::Bytes(48)));
    assert_eq!(
        value(&vpn.values, "last_answer"),
        Some(&Reading::Time(bench.time.unix()))
    );
    assert_eq!(
        value(&vpn.values, "endpoint_address"),
        Some(&Reading::Address("192.0.2.9".parse().unwrap()))
    );
    assert_eq!(
        value(&state.values, "sent_bytes"),
        Some(&Reading::Bytes(48))
    );
    // The same flow is kept as it is.
    assert_eq!(bench.flows.asked.lock().unwrap().len(), 1);

    // Three keepalives without an answer: connecting again, and logged.
    bench.time.pass(Duration::from_secs(7));
    let vpn = bench.converge(&desired).instances["vpn"].clone();
    assert_eq!(
        (vpn.phase, vpn.message.as_str()),
        (Phase::Connecting, "no answer from 192.0.2.9:4000 for 7s")
    );
    assert!(bench.lines().contains(
        &"demo/vpn: tn_vpn is connecting: no answer from 192.0.2.9:4000 for 7s".to_owned()
    ));
}

#[test]
fn the_default_keepalive_is_five_seconds() {
    let mut bench = Bench::new();
    let desired = channel::desired(
        &settings(""),
        &[tunnel("vpn", 0, "endpoint = \"192.0.2.9:4000\"")],
    );
    bench.converge(&desired);
    assert_eq!(bench.flows.kept.lock().unwrap()["vpn"].every, KEEPALIVE);
}

#[test]
fn a_device_or_a_port_that_cannot_be_had_is_an_error_with_why() {
    let mut bench = Bench::with(Kernel {
        refuses: Some("tn_a"),
        ..Kernel::default()
    });
    let desired = channel::desired(
        &settings(""),
        &[
            tunnel("a", 256, "listen_port = 4001"),
            tunnel("b", 256, "listen_port = 1"),
        ],
    );
    let state = bench.converge(&desired);
    assert_eq!(
        (
            state.instances["a"].phase,
            state.instances["a"].message.as_str()
        ),
        (
            Phase::Error,
            "tn_a could not be made: Operation not supported"
        )
    );
    assert_eq!(
        (
            state.instances["b"].phase,
            state.instances["b"].message.as_str()
        ),
        (
            Phase::Error,
            "port 1 could not be opened: Permission denied"
        )
    );
    assert!(bench.lines().contains(
        &"demo/a: tn_a is in error: tn_a could not be made: Operation not supported".to_owned()
    ));
    let errors: Vec<_> = bench
        .log
        .entries()
        .into_iter()
        .filter(|entry| entry.level == routeh_plugin_sdk::Level::Error)
        .map(|entry| entry.program)
        .collect();
    assert_eq!(errors, ["demo/a", "demo/b"]);
}

#[test]
fn simulate_reports_the_phase_it_names() {
    let mut bench = Bench::new();
    let desired = channel::desired(
        &settings(""),
        &[
            tunnel("c", 256, "simulate = \"connecting\""),
            tunnel("d", 256, "simulate = \"down\""),
            tunnel("e", 256, "simulate = \"error\""),
        ],
    );
    let state = bench.converge(&desired);
    for (key, phase) in [
        ("c", Phase::Connecting),
        ("d", Phase::Down),
        ("e", Phase::Error),
    ] {
        let instance = &state.instances[key];
        assert_eq!(
            (instance.phase, instance.message.as_str()),
            (phase, "simulated by its settings")
        );
    }
}

#[test]
fn a_tunnel_that_goes_takes_its_device_and_flow_along() {
    let mut bench = Bench::with(Kernel {
        refuses: Some("tn_b"),
        ..Kernel::default()
    });
    let both = channel::desired(
        &settings(""),
        &[
            tunnel("a", 256, "endpoint = \"192.0.2.9:4000\""),
            tunnel("b", 256, "simulate = \"down\""),
        ],
    );
    bench.converge(&both);
    bench.kernel.asked.lock().unwrap().clear();
    let none = channel::desired(&settings(""), &[]);
    let state = bench.converge(&none);
    assert!(state.instances.is_empty());
    assert_eq!(
        *bench.kernel.asked.lock().unwrap(),
        ["remove tn_a", "remove tn_b"]
    );
    assert!(bench.flows.kept.lock().unwrap().is_empty());
    let lines = bench.lines();
    assert!(lines.contains(&"demo/a: tn_a removed".to_owned()));
    assert!(
        lines.contains(&"demo/b: tn_b could not be removed: Device or resource busy".to_owned())
    );
}

#[test]
fn shutting_down_ends_the_flows_and_keeps_the_devices() {
    let mut bench = Bench::new();
    let desired = channel::desired(
        &settings(""),
        &[tunnel("a", 256, "endpoint = \"192.0.2.9:4000\"")],
    );
    bench.converge(&desired);
    bench.demo.shutdown(&bench.log);
    assert!(bench.flows.kept.lock().unwrap().is_empty());
    assert_eq!(*bench.kernel.asked.lock().unwrap(), ["add tn_a demo"]);
    assert_eq!(
        bench.lines().last().unwrap(),
        "demo: shutting down; the tunnels' devices stay"
    );
}
