//! The tunnels' devices through rtnetlink: a device of a kind is made
//! with `RTM_NEWLINK` and removed with `RTM_DELLINK`, each answered by
//! the kernel's acknowledgement. The messages are those of Linux v6.12
//! `include/uapi/linux/netlink.h` (`struct nlmsghdr`, `NLMSG_ERROR`),
//! `rtnetlink.h` (`struct ifinfomsg`, `RTM_NEWLINK` 16, `RTM_DELLINK` 17)
//! and `if_link.h` (`IFLA_IFNAME` 3, `IFLA_LINKINFO` 18, `IFLA_INFO_KIND`
//! 1), in host order; the demo's module (kmod/demo) registers the kind
//! `demo`.
//! Making the messages and reading the answer are pure; the socket is
//! Linux's.

const RTM_NEWLINK: u16 = 16;
const RTM_DELLINK: u16 = 17;
const NLM_F_REQUEST: u16 = 0x1;
const NLM_F_ACK: u16 = 0x4;
const NLM_F_EXCL: u16 = 0x200;
const NLM_F_CREATE: u16 = 0x400;
const NLMSG_ERROR: u16 = 2;
const IFLA_IFNAME: u16 = 3;
const IFLA_LINKINFO: u16 = 18;
const IFLA_INFO_KIND: u16 = 1;
/// `EEXIST` and `ENODEV` of Linux's `errno-base.h`.
const EEXIST: i32 = 17;
const ENODEV: i32 = 19;

/// One attribute: its length, its type and its payload, padded to four
/// bytes.
fn attribute(bytes: &mut Vec<u8>, kind: u16, payload: &[u8]) {
    let length = 4 + payload.len();
    bytes.extend((length as u16).to_ne_bytes());
    bytes.extend(kind.to_ne_bytes());
    bytes.extend(payload);
    bytes.resize(bytes.len().next_multiple_of(4), 0);
}

/// A text attribute's payload: the text and its terminating NUL.
fn text(value: &str) -> Vec<u8> {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

/// A request of `kind` with `flags` about the device `name`, its
/// `ifinfomsg` all zero (any family, no index: the name says which).
fn request(kind: u16, flags: u16, sequence: u32, name: &str, more: &[u8]) -> Vec<u8> {
    let mut payload = vec![0u8; 16];
    attribute(&mut payload, IFLA_IFNAME, &text(name));
    payload.extend(more);
    let mut bytes = Vec::new();
    bytes.extend(((16 + payload.len()) as u32).to_ne_bytes());
    bytes.extend(kind.to_ne_bytes());
    bytes.extend(flags.to_ne_bytes());
    bytes.extend(sequence.to_ne_bytes());
    bytes.extend(0u32.to_ne_bytes());
    bytes.extend(payload);
    bytes
}

/// `RTM_NEWLINK` of a device of `kind` named `name`, made only if no
/// device has that name.
pub fn new_link(name: &str, kind: &str, sequence: u32) -> Vec<u8> {
    let mut info_kind = Vec::new();
    attribute(&mut info_kind, IFLA_INFO_KIND, &text(kind));
    let mut info = Vec::new();
    attribute(&mut info, IFLA_LINKINFO, &info_kind);
    request(
        RTM_NEWLINK,
        NLM_F_REQUEST | NLM_F_ACK | NLM_F_EXCL | NLM_F_CREATE,
        sequence,
        name,
        &info,
    )
}

/// `RTM_DELLINK` of the device named `name`.
pub fn del_link(name: &str, sequence: u32) -> Vec<u8> {
    request(RTM_DELLINK, NLM_F_REQUEST | NLM_F_ACK, sequence, name, &[])
}

/// The kernel's answer to a request: `Ok` for an error of 0, else the
/// errno; an answer that is no acknowledgement is an errno of 0.
pub fn acknowledged(answer: &[u8]) -> Result<(), i32> {
    if answer.len() < 20 || u16::from_ne_bytes([answer[4], answer[5]]) != NLMSG_ERROR {
        return Err(0);
    }
    match i32::from_ne_bytes([answer[16], answer[17], answer[18], answer[19]]) {
        0 => Ok(()),
        error => Err(-error),
    }
}

/// The kernel's own devices.
pub struct Kernel;

impl Kernel {
    /// Sends `request` and reads the answer; `ignored` is an errno that
    /// is no failure.
    #[cfg(target_os = "linux")]
    fn ask(request: &[u8], ignored: i32) -> Result<(), String> {
        use socket2::{Domain, Protocol, Socket, Type};
        use std::io::{Read, Write};
        const AF_NETLINK: i32 = 16;
        // Unbound and unconnected: the kernel binds it on the first send,
        // and a message with no address goes to the kernel (port 0).
        let mut socket = Socket::new(Domain::from(AF_NETLINK), Type::RAW, Some(Protocol::from(0)))
            .map_err(|error| error.to_string())?;
        socket
            .write_all(request)
            .map_err(|error| error.to_string())?;
        let mut answer = vec![0u8; 4096];
        let read = socket
            .read(&mut answer)
            .map_err(|error| error.to_string())?;
        answer.truncate(read);
        match acknowledged(&answer) {
            Ok(()) => Ok(()),
            Err(errno) if errno == ignored => Ok(()),
            Err(0) => Err("the kernel did not acknowledge".into()),
            Err(errno) => Err(std::io::Error::from_raw_os_error(errno).to_string()),
        }
    }
    #[cfg(not(target_os = "linux"))]
    fn ask(_request: &[u8], _ignored: i32) -> Result<(), String> {
        Err("rtnetlink is Linux's".into())
    }
}

impl crate::Links for Kernel {
    fn add(&mut self, name: &str, kind: &str) -> Result<(), String> {
        Self::ask(&new_link(name, kind, 1), EEXIST)
    }
    fn remove(&mut self, name: &str) -> Result<(), String> {
        Self::ask(&del_link(name, 2), ENODEV)
    }
}
