//! The demo plugin: everything a plugin can do, once, to see and test
//! how the router handles each (README.md).
//!
//! - **Its page**: every type of setting is kept and counted
//!   (`fields_set`); records are rows with values and two buttons each
//!   (`ping` counts, `break` puts the row in error until pressed again);
//!   the page's buttons log, take long or fail.
//! - **Its log**: a line for every `desired`, every change of a tunnel's
//!   phase, every button; a heartbeat when asked for; with `log_level =
//!   "debug"` a line for every converge.
//! - **Settling**: `settle_after` holds back `settled` after a new
//!   `desired`, to see an apply wait (and fail after `settle_within`).
//! - **Its tunnel**: a device of kind `demo` (its module, kmod/demo) for
//!   each instance, by the name the router gave it, removed when the
//!   instance goes. With an endpoint it pings it over UDP from a socket
//!   with the instance's mark and is `connecting` until it is answered;
//!   with a listen port it answers pings. `simulate` reports a phase
//!   instead (`error` fails an apply, `starting` never settles).
//!
//! It keeps nothing across a restart: a device that is there already is
//! taken as made, and a device it was not told of is never touched.
#![forbid(unsafe_code)]
pub mod clock;
pub mod netlink;
pub mod outer;
mod settings;

pub use clock::{Clock, System};
pub use outer::{Flow, FlowStats, Outer, Udp};
pub use settings::{Record, Settings, Tunnel, duration};

use routeh_plugin_sdk::{Action, Desired, InstanceState, Log, Phase, Plugin, RowState, State};
use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// The kind of its tunnels' devices, its module's.
pub const KIND: &str = "demo";
/// How often it converges again: heartbeats, settling and answers are
/// looked at this often.
pub const TICK: Duration = Duration::from_secs(1);
/// A tunnel's keepalive when it has none.
pub const KEEPALIVE: Duration = Duration::from_secs(5);
/// How long the `slow` button takes.
pub const SLOW: Duration = Duration::from_secs(5);
/// How many lines the `log_burst` button logs.
pub const BURST: usize = 100;
/// Keepalives without an answer before a tunnel is `connecting` again.
pub const MISSED: u32 = 3;

/// The kernel's devices, as far as the demo makes them: the boundary its
/// tests replace.
pub trait Links: Send {
    /// Makes a device of `kind` named `name`; one of that name that is
    /// there already is taken as made.
    fn add(&mut self, name: &str, kind: &str) -> Result<(), String>;
    /// Removes the device `name`; one that is not there is gone already.
    fn remove(&mut self, name: &str) -> Result<(), String>;
}

/// One instance as it is now.
struct Made {
    device: String,
    phase: Phase,
    message: String,
}

/// The demo: its devices through `L`, its outer traffic through `O`,
/// time through `C`.
pub struct Demo<L, O, C> {
    links: L,
    outer: O,
    clock: C,
    started: i64,
    /// How many `desired` it was told.
    received: u64,
    /// The last `desired` and when it came.
    digest: Option<(String, Instant)>,
    heartbeats: u64,
    next_beat: Option<Instant>,
    made: BTreeMap<String, Made>,
    /// How often each record was pinged and when last, by host.
    pings: BTreeMap<String, (u64, i64)>,
    broken: BTreeSet<String>,
}

impl Demo<netlink::Kernel, Udp, System> {
    /// The demo with the kernel's devices and sockets.
    pub fn system() -> Self {
        Self::new(netlink::Kernel, Udp::new(), System)
    }
}

impl<L: Links, O: Outer, C: Clock> Demo<L, O, C> {
    pub fn new(links: L, outer: O, clock: C) -> Self {
        let started = clock.unix();
        Self {
            links,
            outer,
            clock,
            started,
            received: 0,
            digest: None,
            heartbeats: 0,
            next_beat: None,
            made: BTreeMap::new(),
            pings: BTreeMap::new(),
            broken: BTreeSet::new(),
        }
    }

    /// A heartbeat when one is due.
    fn beat(&mut self, settings: &Settings, now: Instant, log: &Log) {
        let Some(period) = settings.heartbeat() else {
            self.next_beat = None;
            return;
        };
        match self.next_beat {
            Some(at) if now >= at => {
                self.heartbeats += 1;
                log.info(format!(
                    "heartbeat {}: {}",
                    self.heartbeats,
                    settings.greeting()
                ));
                self.next_beat = Some(now + period);
            }
            Some(_) => {}
            None => self.next_beat = Some(now + period),
        }
    }

    /// Removes the device and flow of each instance that is gone or whose
    /// device is another one now.
    fn remove_gone(&mut self, desired: &Desired<Settings, Tunnel>, log: &Log) {
        let gone: Vec<String> = self
            .made
            .iter()
            .filter(|(key, made)| {
                desired
                    .instances
                    .get(key.as_str())
                    .is_none_or(|wanted| wanted.device != made.device)
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in gone {
            let _ = self.outer.keep(&key, None);
            let Some(made) = self.made.remove(&key) else {
                continue;
            };
            match self.links.remove(&made.device) {
                Ok(()) => log.instance(&key).info(format!("{} removed", made.device)),
                Err(error) => log
                    .instance(&key)
                    .error(format!("{} could not be removed: {error}", made.device)),
            }
        }
    }

    /// Makes an instance's device and flow; its phase and why.
    fn make(&mut self, key: &str, device: &str, mark: u32, tunnel: &Tunnel) -> (Phase, String) {
        if let Err(error) = self.links.add(device, KIND) {
            let _ = self.outer.keep(key, None);
            return (Phase::Error, format!("{device} could not be made: {error}"));
        }
        let every = keepalive(tunnel);
        let flow = Flow {
            endpoint: tunnel.endpoint.clone(),
            mark,
            every,
            listen: tunnel.listen_port,
        };
        let wanted = (flow.endpoint.is_some() || flow.listen.is_some()).then_some(&flow);
        if let Err(error) = self.outer.keep(key, wanted) {
            return (Phase::Error, error);
        }
        if let Some(simulated) = tunnel.simulate.as_deref().and_then(phase) {
            return (simulated, "simulated by its settings".into());
        }
        let Some(endpoint) = &tunnel.endpoint else {
            return (Phase::Up, String::new());
        };
        let quiet = every.as_secs() as i64 * i64::from(MISSED);
        let unix = self.clock.unix();
        match self.outer.stats(key).last_answer {
            None => (Phase::Connecting, format!("no answer from {endpoint} yet")),
            Some(at) if unix - at > quiet => (
                Phase::Connecting,
                format!("no answer from {endpoint} for {}s", unix - at),
            ),
            Some(_) => (Phase::Up, String::new()),
        }
    }

    fn instance_state(&self, key: &str, made: &Made, tunnel: &Tunnel) -> InstanceState {
        let stats = self.outer.stats(key);
        let mut state = InstanceState::new(made.phase)
            .message(made.message.clone())
            .value("sent", stats.sent)
            .value("answered", stats.answered)
            .value("heard", stats.heard)
            .value("sent_bytes", stats.sent_bytes);
        if let Some(endpoint) = &tunnel.endpoint {
            state = state
                .peer(endpoint.clone())
                .value("keepalive", keepalive(tunnel).as_secs());
        }
        if let Some(at) = stats.last_answer {
            state = state.value("last_answer", at);
        }
        if let Some(address) = stats.endpoint_address {
            state = state.value("endpoint_address", address.to_string());
        }
        state
    }

    fn row(&self, record: &Record) -> RowState {
        let (phase, message) = if self.broken.contains(&record.host) {
            (Phase::Error, "broken by its button")
        } else if record.enabled != Some(true) {
            (Phase::Down, "switched off")
        } else {
            (Phase::Up, "")
        };
        let (pings, last) = self.pings.get(&record.host).copied().unwrap_or_default();
        let mut row = RowState::new(phase).message(message).value("pings", pings);
        if pings > 0 {
            row = row.value("last_ping", last);
        }
        let summary = match (&record.target, record.rest.get("port")) {
            (None, _) => "no uplink".to_owned(),
            (Some(target), Some(port)) => format!("through {target}, port {port}"),
            (Some(target), None) => format!("through {target}"),
        };
        row.value("summary", summary)
    }
}

fn keepalive(tunnel: &Tunnel) -> Duration {
    tunnel
        .keepalive
        .as_deref()
        .and_then(duration)
        .unwrap_or(KEEPALIVE)
}

/// A phase `simulate` names.
fn phase(word: &str) -> Option<Phase> {
    Some(match word {
        "starting" => Phase::Starting,
        "connecting" => Phase::Connecting,
        "up" => Phase::Up,
        "down" => Phase::Down,
        "error" => Phase::Error,
        _ => return None,
    })
}

impl<L: Links, O: Outer, C: Clock> Plugin for Demo<L, O, C> {
    const ID: &'static str = "demo";
    /// Set by the build (`ROUTEH_PLUGIN_VERSION`); the crate's otherwise.
    const VERSION: &'static str = match option_env!("ROUTEH_PLUGIN_VERSION") {
        Some(version) => version,
        None => env!("CARGO_PKG_VERSION"),
    };
    type Settings = Settings;
    type Instance = Tunnel;

    fn converge(&mut self, desired: &Desired<Settings, Tunnel>, log: &Log) -> State {
        let settings = &desired.settings;
        let now = self.clock.now();
        if self.digest.as_ref().map(|(digest, _)| digest) != Some(&desired.digest) {
            self.received += 1;
            self.digest = Some((desired.digest.clone(), now));
            log.info(format!(
                "desired {}: {} tunnels, {} records, {} other fields set",
                desired.digest,
                desired.instances.len(),
                settings.records.len(),
                settings.rest.len()
            ));
        } else if settings.debug() {
            log.debug(format!("converging again to {}", desired.digest));
        }
        self.beat(settings, now, log);
        self.remove_gone(desired, log);
        for (key, wanted) in &desired.instances {
            let (phase, message) = self.make(key, &wanted.device, wanted.mark, &wanted.settings);
            let before = self
                .made
                .get(key)
                .map(|made| (made.phase, made.message.clone()));
            if before.as_ref() != Some(&(phase, message.clone())) {
                let log = log.instance(key);
                let is = match phase {
                    Phase::Error => "in error",
                    other => other.word(),
                };
                let line = match message.as_str() {
                    "" => format!("{} is {is}", wanted.device),
                    why => format!("{} is {is}: {why}", wanted.device),
                };
                if phase == Phase::Error {
                    log.error(line);
                } else {
                    log.info(line);
                }
            }
            self.made.insert(
                key.clone(),
                Made {
                    device: wanted.device.clone(),
                    phase,
                    message,
                },
            );
        }
        let hosts: BTreeSet<&str> = settings.records.iter().map(|r| r.host.as_str()).collect();
        self.pings.retain(|host, _| hosts.contains(host.as_str()));
        self.broken.retain(|host| hosts.contains(host.as_str()));

        let since = self.digest.as_ref().map_or(now, |(_, at)| *at);
        let starting = self.made.values().any(|made| made.phase == Phase::Starting);
        let mut state = State::settled()
            .value("greeting", settings.greeting())
            .value("received", self.received)
            .value("heartbeats", self.heartbeats)
            .value("started", self.started)
            .value("fields_set", settings.rest.len() as u64);
        state.settled = !starting && now.duration_since(since) >= settings.settle_after();
        if let Some(period) = settings.heartbeat() {
            state = state.value("heartbeat", period.as_secs());
        }
        if let Some(address) = settings
            .rest
            .get("address")
            .and_then(|value| value.as_str())
            .and_then(|text| text.parse::<IpAddr>().ok())
        {
            state = state.value("address", address.to_string());
        }
        let sent: u64 = self
            .made
            .keys()
            .map(|key| self.outer.stats(key).sent_bytes)
            .sum();
        state = state.value("sent_bytes", sent);
        for (key, made) in &self.made {
            let tunnel = &desired.instances[key].settings;
            state = state.instance(key, self.instance_state(key, made, tunnel));
        }
        for record in &settings.records {
            state = state.row("records", &record.host, self.row(record));
        }
        state
    }

    fn tick(&self) -> Option<Duration> {
        Some(TICK)
    }

    fn action(
        &mut self,
        action: &Action,
        desired: Option<&Desired<Settings, Tunnel>>,
        log: &Log,
    ) -> Result<String, String> {
        let settings = desired.map(|desired| &desired.settings);
        let record = || {
            let host = action.row.as_deref().unwrap_or_default();
            settings
                .and_then(|settings| settings.records.iter().find(|r| r.host == host))
                .map(|record| record.host.clone())
                .ok_or_else(|| format!("no record {host}"))
        };
        match action.key.as_str() {
            "hello" => {
                let greeting = settings.map_or("Hello", Settings::greeting);
                log.notice(greeting);
                Ok(format!("Logged \"{greeting}\""))
            }
            "log_burst" => {
                for line in 1..=BURST {
                    let text = format!("burst line {line} of {BURST}");
                    match line % 5 {
                        0 => log.error(text),
                        1 => log.warning(text),
                        2 => log.notice(text),
                        3 => log.info(text),
                        _ => log.debug(text),
                    }
                }
                Ok(format!("Logged {BURST} lines, {} of each level", BURST / 5))
            }
            "slow" => {
                log.info(format!("taking {}s", SLOW.as_secs()));
                self.clock.sleep(SLOW);
                Ok(format!("Done after {}s", SLOW.as_secs()))
            }
            "fail" => {
                log.warning("the Fail button was pressed");
                Err("This button always fails".into())
            }
            "ping" => {
                let host = record()?;
                let unix = self.clock.unix();
                let entry = self.pings.entry(host.clone()).or_default();
                *entry = (entry.0 + 1, unix);
                log.info(format!("ping {host}"));
                Ok(format!("pong {host}"))
            }
            "break" => {
                let host = record()?;
                if self.broken.remove(&host) {
                    log.info(format!("{host} mended"));
                    Ok(format!("{host} is mended"))
                } else {
                    log.warning(format!("{host} broken"));
                    self.broken.insert(host.clone());
                    Ok(format!("{host} is broken"))
                }
            }
            other => Err(format!("{other} is not an action of the demo")),
        }
    }

    fn shutdown(&mut self, log: &Log) {
        for key in self.made.keys() {
            let _ = self.outer.keep(key, None);
        }
        log.info("shutting down; the tunnels' devices stay");
    }
}
