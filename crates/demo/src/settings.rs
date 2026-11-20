//! What `desired` tells the demo, read into its own types: its settings
//! (`[plugins.demo]`) and a tunnel's (`[tunnels.<key>.demo]`), each value
//! as the document writes it (a duration `"25s"`).
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

/// The demo's own settings. The fields that change what it does are
/// read; every other field of the manifest is kept in `rest`, only
/// counted and echoed.
#[derive(Debug, Default, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub greeting: Option<String>,
    /// `info` or `debug`: with `debug` every converge is logged.
    #[serde(default)]
    pub log_level: Option<String>,
    /// A line in the log this often.
    #[serde(default)]
    pub heartbeat: Option<String>,
    /// How long after a new `desired` it says it settled.
    #[serde(default)]
    pub settle_after: Option<String>,
    #[serde(default)]
    pub records: Vec<Record>,
    #[serde(flatten)]
    pub rest: BTreeMap<String, Value>,
}

impl Settings {
    pub fn greeting(&self) -> &str {
        self.greeting.as_deref().unwrap_or("Hello")
    }
    pub fn debug(&self) -> bool {
        self.log_level.as_deref() == Some("debug")
    }
    pub fn heartbeat(&self) -> Option<Duration> {
        self.heartbeat.as_deref().and_then(duration)
    }
    pub fn settle_after(&self) -> Duration {
        self.settle_after
            .as_deref()
            .and_then(duration)
            .unwrap_or_default()
    }
}

/// A record of the page's group, named by its host.
#[derive(Debug, Deserialize)]
pub struct Record {
    pub host: String,
    /// The key of an uplink.
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub rest: BTreeMap<String, Value>,
}

/// A tunnel's settings as `desired` tells them (the addresses are
/// netifd's and are left out).
#[derive(Debug, Default, Deserialize)]
pub struct Tunnel {
    /// `host:port` to ping.
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub keepalive: Option<String>,
    /// A UDP port where it answers pings.
    #[serde(default)]
    pub listen_port: Option<u16>,
    /// A phase to report instead of the one it is in.
    #[serde(default)]
    pub simulate: Option<String>,
}

/// A duration as the document writes it: a whole number and `s`, `m`
/// or `h`.
pub fn duration(text: &str) -> Option<Duration> {
    let unit = match text.chars().last()? {
        's' => 1,
        'm' => 60,
        'h' => 3600,
        _ => return None,
    };
    let number = &text[..text.len() - 1];
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    number
        .parse::<u64>()
        .ok()?
        .checked_mul(unit)
        .map(Duration::from_secs)
}
