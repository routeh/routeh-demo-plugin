//! A tunnel's outer traffic over the kernel's UDP sockets, as a real
//! tunnel's: a ping to its endpoint every keepalive from a socket with
//! the tunnel's mark (`SO_MARK`, socket(7), which needs `CAP_NET_ADMIN`;
//! a mark of 0 is not set), the answers to it counted, and, with a
//! listen port, an answer to every ping heard there. Each runs on a
//! thread of its own until its part of the flow changes or the flow ends.
use crate::clock::unix_now;
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What a ping says.
pub const PING: &[u8] = b"routeh-demo ping";
/// What its answer says.
pub const ANSWER: &[u8] = b"routeh-demo answer";
/// How long a socket waits for a datagram before it looks whether its
/// flow ended.
const WAIT: Duration = Duration::from_millis(200);

/// What a tunnel's outer traffic is to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flow {
    /// `host:port` to ping.
    pub endpoint: Option<String>,
    /// The mark of the socket that pings.
    pub mark: u32,
    pub every: Duration,
    /// The UDP port it answers pings on.
    pub listen: Option<u16>,
}

/// What a flow did since it began.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlowStats {
    pub sent: u64,
    pub sent_bytes: u64,
    pub answered: u64,
    pub heard: u64,
    /// Seconds since 1970 UTC.
    pub last_answer: Option<i64>,
    /// What the endpoint was resolved to.
    pub endpoint_address: Option<IpAddr>,
}

/// The tunnels' outer traffic: the boundary its tests replace.
pub trait Outer: Send {
    /// Keeps the flow of the tunnel `key`: one that is the same goes on
    /// as it is, another replaces it, `None` ends it.
    fn keep(&mut self, key: &str, flow: Option<&Flow>) -> Result<(), String>;
    /// What the flow of `key` did; nothing for one it does not have.
    fn stats(&self, key: &str) -> FlowStats;
}

/// A thread of a flow: told to stop and waited for when dropped, so that
/// what it holds (a listen port) is free once it is gone.
struct Worker {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Worker {
    fn spawn(work: impl FnOnce(&AtomicBool) + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || work(&flag));
        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A tunnel's flow: its listener and its pinger each restarted only when
/// their part of the flow changes, so a listen port that stays is never
/// opened again (and never found still held by the thread before).
struct Running {
    flow: Flow,
    stats: Arc<Mutex<FlowStats>>,
    listener: Option<Worker>,
    pinger: Option<Worker>,
}

/// The kernel's sockets.
#[derive(Default)]
pub struct Udp {
    flows: BTreeMap<String, Running>,
}

impl Udp {
    pub fn new() -> Self {
        Self::default()
    }
}

fn lock(stats: &Mutex<FlowStats>) -> std::sync::MutexGuard<'_, FlowStats> {
    stats
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn listener(port: u16, stats: &Arc<Mutex<FlowStats>>) -> Result<Worker, String> {
    let socket = UdpSocket::bind(("0.0.0.0", port))
        .map_err(|error| format!("port {port} could not be opened: {error}"))?;
    socket
        .set_read_timeout(Some(WAIT))
        .map_err(|error| error.to_string())?;
    let stats = stats.clone();
    Ok(Worker::spawn(move |stop| answer(&socket, stop, &stats)))
}

fn pinger(flow: &Flow, endpoint: &str, stats: &Arc<Mutex<FlowStats>>) -> Result<Worker, String> {
    let to = endpoint
        .to_socket_addrs()
        .map_err(|error| format!("{endpoint} does not resolve: {error}"))?
        .next()
        .ok_or_else(|| format!("{endpoint} has no address"))?;
    let socket = socket(to, flow.mark)?;
    lock(stats).endpoint_address = Some(to.ip());
    let (stats, every) = (stats.clone(), flow.every);
    Ok(Worker::spawn(move |stop| {
        ping(&socket, every, stop, &stats)
    }))
}

impl Outer for Udp {
    fn keep(&mut self, key: &str, flow: Option<&Flow>) -> Result<(), String> {
        let Some(flow) = flow else {
            self.flows.remove(key);
            return Ok(());
        };
        if self
            .flows
            .get(key)
            .is_some_and(|running| &running.flow == flow)
        {
            return Ok(());
        }
        let mut running = self.flows.remove(key).unwrap_or_else(|| Running {
            flow: flow.clone(),
            stats: Arc::default(),
            listener: None,
            pinger: None,
        });
        let first = running.listener.is_none() && running.pinger.is_none();
        let same_listen = !first && running.flow.listen == flow.listen;
        let same_ping = !first
            && running.flow.endpoint == flow.endpoint
            && running.flow.mark == flow.mark
            && running.flow.every == flow.every;
        // Another flow begins anew: its counts too. A pinger that goes on
        // keeps what its endpoint resolved to.
        {
            let mut stats = lock(&running.stats);
            let address = stats.endpoint_address.filter(|_| same_ping);
            *stats = FlowStats {
                endpoint_address: address,
                ..FlowStats::default()
            };
        }
        if !same_ping {
            running.pinger = None;
        }
        if !same_listen {
            running.listener = None;
            if let Some(port) = flow.listen {
                // On an error `running` is dropped: the flow ends whole.
                running.listener = Some(listener(port, &running.stats)?);
            }
        }
        if !same_ping && let Some(endpoint) = &flow.endpoint {
            running.pinger = Some(pinger(flow, endpoint, &running.stats)?);
        }
        running.flow = flow.clone();
        self.flows.insert(key.to_owned(), running);
        Ok(())
    }

    fn stats(&self, key: &str) -> FlowStats {
        self.flows
            .get(key)
            .map(|running| *lock(&running.stats))
            .unwrap_or_default()
    }
}

/// Answers every ping heard on `socket` until `stop`.
fn answer(socket: &UdpSocket, stop: &AtomicBool, stats: &Mutex<FlowStats>) {
    let mut buffer = [0u8; 64];
    while !stop.load(Ordering::Relaxed) {
        if let Ok((read, from)) = socket.recv_from(&mut buffer)
            && &buffer[..read] == PING
        {
            lock(stats).heard += 1;
            let _ = socket.send_to(ANSWER, from);
        }
    }
}

/// Pings through `socket` every `every` and counts the answers, until
/// `stop`.
fn ping(socket: &UdpSocket, every: Duration, stop: &AtomicBool, stats: &Mutex<FlowStats>) {
    let mut buffer = [0u8; 64];
    let mut next = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        if Instant::now() >= next {
            // A ping that does not leave is tried again next time.
            if socket.send(PING).is_ok() {
                let mut stats = lock(stats);
                stats.sent += 1;
                stats.sent_bytes += PING.len() as u64;
            }
            next = Instant::now() + every;
        }
        if let Ok(read) = socket.recv(&mut buffer)
            && &buffer[..read] == ANSWER
        {
            let mut stats = lock(stats);
            stats.answered += 1;
            stats.last_answer = Some(unix_now());
        }
    }
}

/// A socket marked `mark`, connected to `to`, that waits [`WAIT`].
fn socket(to: SocketAddr, mark: u32) -> Result<UdpSocket, String> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::for_address(to), Type::DGRAM, Some(Protocol::UDP))
        .map_err(|error| error.to_string())?;
    if mark != 0 {
        marked(&socket, mark)?;
    }
    socket
        .connect(&to.into())
        .map_err(|error| error.to_string())?;
    let socket: UdpSocket = socket.into();
    socket
        .set_read_timeout(Some(WAIT))
        .map_err(|error| error.to_string())?;
    Ok(socket)
}

#[cfg(target_os = "linux")]
fn marked(socket: &socket2::Socket, mark: u32) -> Result<(), String> {
    socket
        .set_mark(mark)
        .map_err(|error| format!("its mark could not be set: {error}"))
}

#[cfg(not(target_os = "linux"))]
fn marked(_: &socket2::Socket, _: u32) -> Result<(), String> {
    Err("marks need Linux".into())
}
