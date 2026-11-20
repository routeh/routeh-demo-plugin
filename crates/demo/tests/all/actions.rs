//! The page's buttons and the records'.
use crate::support::{Bench, read, settings, value};
use routeh_plugin_sdk::protocol::channel::{self, Phase, Reading};
use routeh_plugin_sdk::{Action, Level, Plugin};
use std::time::Duration;

fn press(
    bench: &mut Bench,
    desired: &channel::Desired,
    key: &str,
    row: Option<&str>,
) -> Result<String, String> {
    let action = Action {
        id: 1,
        key: key.into(),
        row: row.map(Into::into),
    };
    bench.demo.action(&action, Some(&read(desired)), &bench.log)
}

#[test]
fn hello_logs_the_greeting_as_a_notice() {
    let mut bench = Bench::new();
    let desired = channel::desired(&settings("greeting = \"Hi there\""), &[]);
    assert_eq!(
        press(&mut bench, &desired, "hello", None),
        Ok("Logged \"Hi there\"".into())
    );
    let last = bench.log.entries().pop().unwrap();
    assert_eq!(
        (last.level, last.text.as_str()),
        (Level::Notice, "Hi there")
    );
}

#[test]
fn log_burst_logs_a_hundred_lines_of_every_level() {
    let mut bench = Bench::new();
    let desired = channel::desired(&settings(""), &[]);
    assert_eq!(
        press(&mut bench, &desired, "log_burst", None),
        Ok("Logged 100 lines, 20 of each level".into())
    );
    let entries = bench.log.entries();
    assert_eq!(entries.len(), 100);
    for level in [
        Level::Error,
        Level::Warning,
        Level::Notice,
        Level::Info,
        Level::Debug,
    ] {
        assert_eq!(
            entries.iter().filter(|e| e.level == level).count(),
            20,
            "{level:?}"
        );
    }
    assert_eq!(entries[99].text, "burst line 100 of 100");
}

#[test]
fn slow_takes_five_seconds_and_fail_fails() {
    let mut bench = Bench::new();
    let desired = channel::desired(&settings(""), &[]);
    let before = bench.time.unix();
    assert_eq!(
        press(&mut bench, &desired, "slow", None),
        Ok("Done after 5s".into())
    );
    assert_eq!(bench.time.unix() - before, 5);
    assert_eq!(
        press(&mut bench, &desired, "fail", None),
        Err("This button always fails".into())
    );
    assert_eq!(
        press(&mut bench, &desired, "launch", None),
        Err("launch is not an action of the demo".into())
    );
}

#[test]
fn ping_counts_and_dates_a_record_and_break_puts_it_in_error_until_pressed_again() {
    let mut bench = Bench::new();
    let desired = channel::desired(
        &settings("records = [{ host = \"home\" }, { host = \"lab\" }]"),
        &[],
    );
    bench.converge(&desired);
    bench.time.pass(Duration::from_secs(7));
    assert_eq!(
        press(&mut bench, &desired, "ping", Some("home")),
        Ok("pong home".into())
    );
    let state = bench.converge(&desired);
    let home = &state.rows["records"]["home"];
    assert_eq!(value(&home.values, "pings"), Some(&Reading::Count(1)));
    assert_eq!(
        value(&home.values, "last_ping"),
        Some(&Reading::Time(bench.time.unix()))
    );
    assert_eq!(
        value(&state.rows["records"]["lab"].values, "pings"),
        Some(&Reading::Count(0))
    );
    assert_eq!(
        press(&mut bench, &desired, "ping", Some("gone")),
        Err("no record gone".into())
    );

    assert_eq!(
        press(&mut bench, &desired, "break", Some("lab")),
        Ok("lab is broken".into())
    );
    let lab = bench.converge(&desired).rows["records"]["lab"].clone();
    assert_eq!(
        (lab.phase, lab.message.as_str()),
        (Phase::Error, "broken by its button")
    );
    assert_eq!(
        press(&mut bench, &desired, "break", Some("lab")),
        Ok("lab is mended".into())
    );
    assert_eq!(
        bench.converge(&desired).rows["records"]["lab"].phase,
        Phase::Up
    );

    // A record that is deleted takes its count and its break along.
    press(&mut bench, &desired, "break", Some("home")).unwrap();
    bench.converge(&channel::desired(
        &settings("records = [{ host = \"lab\" }]"),
        &[],
    ));
    let state = bench.converge(&desired);
    let home = &state.rows["records"]["home"];
    assert_eq!(home.phase, Phase::Up);
    assert_eq!(value(&home.values, "pings"), Some(&Reading::Count(0)));
}
