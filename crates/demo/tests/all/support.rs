//! The demo's boundaries in memory, its manifest, and `desired` made as
//! the router makes it.
use demo::{Clock, Demo, Flow, FlowStats, Links, Outer};
use routeh_plugin_sdk::protocol::channel::{self, Report};
use routeh_plugin_sdk::protocol::{Manifest, Settings};
use routeh_plugin_sdk::{Desired, Log, Plugin};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The manifest as the bundle carries it: the build adds the version of
/// the kernel its module was built for.
pub fn manifest() -> Manifest {
    let written = include_str!("../../../../plugin/manifest.toml");
    let bundled = written.replacen(
        "load = [\"demo\"]",
        "load = [\"demo\"]\nkernel = \"6.12.112~67e205f26d0458c4d12f1fd315216398-r1\"",
        1,
    );
    assert_ne!(bundled, written, "the manifest loads its module");
    Manifest::parse(&bundled).expect("plugin/manifest.toml is a manifest the router installs")
}

/// The plugin's settings as the router checks them.
pub fn settings(text: &str) -> Settings {
    manifest()
        .settings
        .check(&text.parse().unwrap())
        .expect("settings the manifest takes")
}

/// A tunnel of the demo's kind with these settings, checked by the
/// manifest as the router checks them (the address is required, and is
/// netifd's: it is not told).
pub fn tunnel(key: &str, mark: u32, text: &str) -> channel::Instance {
    let text = format!("address = \"10.9.0.1/24\"\n{text}");
    let kind = manifest().tunnel.unwrap();
    kind.settings
        .check(&text.parse().unwrap())
        .expect("tunnel settings the manifest takes");
    let mut table: toml::Table = text.parse().unwrap();
    table.remove("address");
    table.remove("address6");
    channel::Instance {
        key: key.into(),
        device: format!("tn_{key}"),
        mark,
        settings: table,
    }
}

/// What the router sends, as the plugin reads it.
pub fn read(desired: &channel::Desired) -> Desired<demo::Settings, demo::Tunnel> {
    Desired::read(desired.text.trim_end().as_bytes()).unwrap()
}

/// The kernel's devices, in memory: what it was asked, and a device it
/// cannot make or remove.
#[derive(Clone, Default)]
pub struct Kernel {
    pub asked: Arc<Mutex<Vec<String>>>,
    pub refuses: Option<&'static str>,
}

impl Links for Kernel {
    fn add(&mut self, name: &str, kind: &str) -> Result<(), String> {
        self.asked
            .lock()
            .unwrap()
            .push(format!("add {name} {kind}"));
        match self.refuses {
            Some(refused) if refused == name => Err("Operation not supported".into()),
            _ => Ok(()),
        }
    }
    fn remove(&mut self, name: &str) -> Result<(), String> {
        self.asked.lock().unwrap().push(format!("remove {name}"));
        match self.refuses {
            Some(refused) if refused == name => Err("Device or resource busy".into()),
            _ => Ok(()),
        }
    }
}

/// The outer flows, in memory: what each is, and what it did as a test
/// sets it.
#[derive(Clone, Default)]
pub struct Flows {
    pub kept: Arc<Mutex<BTreeMap<String, Flow>>>,
    pub asked: Arc<Mutex<Vec<String>>>,
    pub stats: Arc<Mutex<BTreeMap<String, FlowStats>>>,
}

impl Outer for Flows {
    fn keep(&mut self, key: &str, flow: Option<&Flow>) -> Result<(), String> {
        let mut kept = self.kept.lock().unwrap();
        if kept.get(key) == flow {
            return Ok(());
        }
        self.asked.lock().unwrap().push(match flow {
            Some(flow) => format!("{key} {flow:?}"),
            None => format!("{key} ends"),
        });
        match flow {
            Some(flow) if flow.listen == Some(1) => {
                kept.remove(key);
                Err("port 1 could not be opened: Permission denied".into())
            }
            Some(flow) => {
                kept.insert(key.into(), flow.clone());
                Ok(())
            }
            None => {
                kept.remove(key);
                Ok(())
            }
        }
    }
    fn stats(&self, key: &str) -> FlowStats {
        if !self.kept.lock().unwrap().contains_key(key) {
            return FlowStats::default();
        }
        self.stats
            .lock()
            .unwrap()
            .get(key)
            .copied()
            .unwrap_or_default()
    }
}

/// Time that moves only when a test moves it; a sleep moves it.
#[derive(Clone)]
pub struct Time(pub Arc<Mutex<(Instant, i64)>>);

impl Default for Time {
    fn default() -> Self {
        Self(Arc::new(Mutex::new((Instant::now(), 1_800_000_000))))
    }
}

impl Time {
    pub fn pass(&self, period: Duration) {
        let mut now = self.0.lock().unwrap();
        now.0 += period;
        now.1 += period.as_secs() as i64;
    }
    pub fn unix(&self) -> i64 {
        self.0.lock().unwrap().1
    }
}

impl Clock for Time {
    fn now(&self) -> Instant {
        self.0.lock().unwrap().0
    }
    fn unix(&self) -> i64 {
        Time::unix(self)
    }
    fn sleep(&self, period: Duration) {
        self.pass(period);
    }
}

pub type Test = Demo<Kernel, Flows, Time>;

/// The demo with every boundary in memory, and its log.
pub struct Bench {
    pub demo: Test,
    pub kernel: Kernel,
    pub flows: Flows,
    pub time: Time,
    pub log: Log,
}

impl Bench {
    pub fn new() -> Self {
        Self::with(Kernel::default())
    }
    pub fn with(kernel: Kernel) -> Self {
        let (flows, time) = (Flows::default(), Time::default());
        Self {
            demo: Demo::new(kernel.clone(), flows.clone(), time.clone()),
            kernel,
            flows,
            time,
            log: Log::memory("demo"),
        }
    }

    /// Converges to `desired` and reads the state as the router does.
    pub fn converge(&mut self, desired: &channel::Desired) -> Report {
        let state = self.demo.converge(&read(desired), &self.log);
        let line = state.line(Some(&desired.digest));
        match channel::read(line.trim_end().as_bytes(), &manifest()).unwrap() {
            channel::Message::State(report) => {
                assert!(
                    report.dropped.is_empty(),
                    "undeclared: {:?}",
                    report.dropped
                );
                assert!(
                    report.wrong.is_empty(),
                    "of the wrong type: {:?}",
                    report.wrong
                );
                report
            }
            other => panic!("not a state: {other:?}"),
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.log
            .entries()
            .iter()
            .map(|entry| format!("{}: {}", entry.program, entry.text))
            .collect()
    }
}

/// The value of `key` among `values`.
pub fn value<'a>(
    values: &'a [(String, channel::Reading)],
    key: &str,
) -> Option<&'a channel::Reading> {
    values.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}
