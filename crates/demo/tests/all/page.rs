//! The page: the plugin's values, the records' rows, and the log.
use crate::manifest::EVERY_FIELD;
use crate::support::{Bench, settings, value};
use routeh_plugin_sdk::protocol::channel::{self, Phase, Reading};
use std::time::Duration;

#[test]
fn every_field_is_taken_counted_and_one_of_each_value_is_reported() {
    let mut bench = Bench::new();
    let desired = channel::desired(&settings(EVERY_FIELD), &[]);
    let state = bench.converge(&desired);
    // It settles 2 s after (settling.rs).
    assert!(!state.settled);
    assert_eq!(
        value(&state.values, "greeting"),
        Some(&Reading::Text("Hi".into()))
    );
    assert_eq!(value(&state.values, "converged"), Some(&Reading::Count(1)));
    assert_eq!(value(&state.values, "heartbeats"), Some(&Reading::Count(0)));
    assert_eq!(
        value(&state.values, "heartbeat"),
        Some(&Reading::Duration(30))
    );
    assert_eq!(
        value(&state.values, "started"),
        Some(&Reading::Time(bench.time.unix()))
    );
    // Every field but the four it acts on and the records.
    assert_eq!(
        value(&state.values, "fields_set"),
        Some(&Reading::Count(23))
    );
    assert_eq!(
        value(&state.values, "address"),
        Some(&Reading::Address("192.0.2.7".parse().unwrap()))
    );
    assert_eq!(value(&state.values, "sent_bytes"), Some(&Reading::Bytes(0)));
    let home = &state.rows["records"]["home"];
    assert_eq!(home.phase, Phase::Up);
    assert_eq!(value(&home.values, "pings"), Some(&Reading::Count(0)));
    assert_eq!(value(&home.values, "last_ping"), None);
    assert_eq!(
        value(&home.values, "target"),
        Some(&Reading::Text("wan".into()))
    );
}

#[test]
fn with_nothing_set_it_greets_with_hello_and_reports_no_optional_value() {
    let mut bench = Bench::new();
    let state = bench.converge(&channel::desired(&settings(""), &[]));
    assert!(state.settled);
    assert_eq!(
        value(&state.values, "greeting"),
        Some(&Reading::Text("Hello".into()))
    );
    assert_eq!(value(&state.values, "fields_set"), Some(&Reading::Count(0)));
    for absent in ["heartbeat", "address"] {
        assert_eq!(value(&state.values, absent), None, "{absent}");
    }
    assert!(state.rows.get("records").is_none_or(|rows| rows.is_empty()));
}

#[test]
fn a_record_switched_off_is_down() {
    let mut bench = Bench::new();
    let desired = channel::desired(
        &settings("records = [{ host = \"nas\", enabled = false }, { host = \"tv\" }]"),
        &[],
    );
    let state = bench.converge(&desired);
    let rows = &state.rows["records"];
    assert_eq!(
        (rows["nas"].phase, rows["nas"].message.as_str()),
        (Phase::Down, "switched off")
    );
    assert_eq!(rows["tv"].phase, Phase::Up);
}

#[test]
fn each_desired_is_logged_once_and_with_debug_every_converge() {
    let mut bench = Bench::new();
    let quiet = channel::desired(&settings("greeting = \"Hi\""), &[]);
    bench.converge(&quiet);
    bench.converge(&quiet);
    assert_eq!(
        bench.lines(),
        [format!(
            "demo: desired {}: 0 tunnels, 0 records, 0 other fields set",
            quiet.digest
        )]
    );
    let chatty = channel::desired(&settings("log_level = \"debug\""), &[]);
    bench.converge(&chatty);
    let state = bench.converge(&chatty);
    assert_eq!(value(&state.values, "converged"), Some(&Reading::Count(2)));
    assert_eq!(
        bench.lines().last().unwrap(),
        &format!("demo: converging again to {}", chatty.digest)
    );
}

#[test]
fn a_heartbeat_is_logged_every_period_and_counted() {
    let mut bench = Bench::new();
    let desired = channel::desired(&settings("heartbeat = \"10s\"\ngreeting = \"Hi\""), &[]);
    bench.converge(&desired);
    bench.time.pass(Duration::from_secs(9));
    bench.converge(&desired);
    assert!(!bench.lines().iter().any(|line| line.contains("heartbeat")));
    bench.time.pass(Duration::from_secs(1));
    let state = bench.converge(&desired);
    assert_eq!(value(&state.values, "heartbeats"), Some(&Reading::Count(1)));
    assert_eq!(bench.lines().last().unwrap(), "demo: heartbeat 1: Hi");
    bench.time.pass(Duration::from_secs(10));
    let state = bench.converge(&desired);
    assert_eq!(value(&state.values, "heartbeats"), Some(&Reading::Count(2)));
    // Without a heartbeat none comes, and one set again begins anew.
    let without = channel::desired(&settings(""), &[]);
    bench.time.pass(Duration::from_secs(60));
    bench.converge(&without);
    bench.converge(&desired);
    let state = bench.converge(&desired);
    assert_eq!(value(&state.values, "heartbeats"), Some(&Reading::Count(2)));
}
