//! The kernel's side: rtnetlink's messages, and UDP over loopback.
use demo::{Flow, Outer, Udp, netlink};
use std::time::{Duration, Instant};

#[test]
fn netlink_messages_make_a_device_of_a_kind_and_remove_it() {
    // struct nlmsghdr, struct ifinfomsg, IFLA_IFNAME and the nested
    // IFLA_LINKINFO with IFLA_INFO_KIND (Linux v6.12
    // include/uapi/linux/{netlink,rtnetlink,if_link}.h), host order.
    let made = netlink::new_link("tn_a", "demo", 7);
    let mut expected = Vec::new();
    expected.extend(60u32.to_ne_bytes());
    expected.extend(16u16.to_ne_bytes()); // RTM_NEWLINK
    expected.extend(0x0605u16.to_ne_bytes()); // REQUEST | ACK | EXCL | CREATE
    expected.extend(7u32.to_ne_bytes());
    expected.extend(0u32.to_ne_bytes());
    expected.extend([0u8; 16]);
    expected.extend(9u16.to_ne_bytes());
    expected.extend(3u16.to_ne_bytes()); // IFLA_IFNAME
    expected.extend(b"tn_a\0\0\0\0");
    expected.extend(16u16.to_ne_bytes());
    expected.extend(18u16.to_ne_bytes()); // IFLA_LINKINFO
    expected.extend(9u16.to_ne_bytes());
    expected.extend(1u16.to_ne_bytes()); // IFLA_INFO_KIND
    expected.extend(b"demo\0\0\0\0");
    assert_eq!(made, expected);
    let removed = netlink::del_link("tn_a", 8);
    assert_eq!(&removed[4..6], &17u16.to_ne_bytes()); // RTM_DELLINK
    assert_eq!(&removed[6..8], &0x0005u16.to_ne_bytes()); // REQUEST | ACK
    assert_eq!(removed.len(), 44);
    assert_eq!(&removed[..4], &44u32.to_ne_bytes());
    // The kernel's answer: NLMSG_ERROR with an error of 0, or -errno.
    let answer = |error: i32| {
        let mut bytes = Vec::new();
        bytes.extend(36u32.to_ne_bytes());
        bytes.extend(2u16.to_ne_bytes());
        bytes.extend(0u16.to_ne_bytes());
        bytes.extend(7u32.to_ne_bytes());
        bytes.extend(0u32.to_ne_bytes());
        bytes.extend(error.to_ne_bytes());
        bytes.extend([0u8; 16]);
        bytes
    };
    assert_eq!(netlink::acknowledged(&answer(0)), Ok(()));
    assert_eq!(netlink::acknowledged(&answer(-17)), Err(17));
    assert_eq!(netlink::acknowledged(&answer(0)[..10]), Err(0));
    let mut other = answer(0);
    other[4] = 3;
    assert_eq!(netlink::acknowledged(&other), Err(0));
}

/// A port nothing listens on now.
fn free_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "{what} did not happen");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn two_flows_over_loopback_ping_and_answer_each_other() {
    let port = free_port();
    let mut udp = Udp::new();
    let server = Flow {
        endpoint: None,
        mark: 0,
        every: Duration::from_secs(1),
        listen: Some(port),
    };
    let client = Flow {
        endpoint: Some(format!("127.0.0.1:{port}")),
        mark: 0,
        every: Duration::from_millis(20),
        listen: None,
    };
    udp.keep("server", Some(&server)).unwrap();
    udp.keep("client", Some(&client)).unwrap();
    until("two answers", || udp.stats("client").answered >= 2);
    let stats = udp.stats("client");
    assert!(stats.sent >= 2);
    assert_eq!(
        stats.sent_bytes,
        stats.sent * demo::outer::PING.len() as u64
    );
    assert!(stats.last_answer.is_some());
    assert_eq!(stats.endpoint_address, Some("127.0.0.1".parse().unwrap()));
    assert!(udp.stats("server").heard >= 2);

    // The same flow goes on; another begins anew; none ends it.
    udp.keep("client", Some(&client)).unwrap();
    assert!(udp.stats("client").sent >= 2);
    udp.keep("client", None).unwrap();
    assert_eq!(udp.stats("client"), Default::default());
    // The port is free again once its flow ended.
    udp.keep("server", None).unwrap();
    until("the port to be free", || {
        std::net::UdpSocket::bind(("0.0.0.0", port)).is_ok()
    });
}

#[test]
fn a_listen_port_that_stays_is_kept_when_the_rest_of_the_flow_changes() {
    let port = free_port();
    let mut udp = Udp::new();
    let flow = Flow {
        endpoint: Some(format!("127.0.0.1:{port}")),
        mark: 0,
        every: Duration::from_millis(20),
        listen: Some(port),
    };
    udp.keep("a", Some(&flow)).unwrap();
    until("its own pings answered", || udp.stats("a").answered >= 1);
    // Another keepalive, the same port: not opened again, never in use.
    let slower = Flow {
        every: Duration::from_millis(30),
        ..flow.clone()
    };
    udp.keep("a", Some(&slower)).unwrap();
    until("answers again", || udp.stats("a").answered >= 1);
    // No endpoint any more: the listener goes on and still answers.
    let listen_only = Flow {
        endpoint: None,
        ..slower
    };
    udp.keep("a", Some(&listen_only)).unwrap();
    assert_eq!(udp.stats("a").endpoint_address, None);
    // Another port: the old one is free as soon as keep returns.
    let other = Flow {
        listen: Some(free_port()),
        ..listen_only
    };
    udp.keep("a", Some(&other)).unwrap();
    std::net::UdpSocket::bind(("0.0.0.0", port)).expect("the old port is free");
}

#[test]
fn a_port_in_use_or_an_endpoint_that_does_not_resolve_is_said() {
    let taken = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let mut udp = Udp::new();
    let listen = Flow {
        endpoint: None,
        mark: 0,
        every: Duration::from_secs(1),
        listen: Some(port),
    };
    let error = udp.keep("a", Some(&listen)).unwrap_err();
    assert!(
        error.starts_with(&format!("port {port} could not be opened: ")),
        "{error}"
    );
    let nowhere = Flow {
        endpoint: Some("no port".into()),
        ..listen.clone()
    };
    let nowhere = Flow {
        listen: None,
        ..nowhere
    };
    let error = udp.keep("b", Some(&nowhere)).unwrap_err();
    assert!(error.starts_with("no port does not resolve: "), "{error}");
    assert_eq!(udp.stats("b"), Default::default());
}

#[cfg(target_os = "linux")]
#[test]
fn a_mark_needs_cap_net_admin() {
    // Without it (a test is not root) the kernel refuses SO_MARK, and the
    // flow says why.
    if rustix_is_root() {
        return;
    }
    let mut udp = Udp::new();
    let flow = Flow {
        endpoint: Some("127.0.0.1:9".into()),
        mark: 257,
        every: Duration::from_secs(1),
        listen: None,
    };
    let error = udp.keep("a", Some(&flow)).unwrap_err();
    assert!(error.starts_with("its mark could not be set: "), "{error}");
}

#[cfg(target_os = "linux")]
fn rustix_is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .map(|status| status.lines().any(|line| line.starts_with("Uid:\t0\t")))
        .unwrap_or(false)
}
