# routeh demo plugin

Everything a routeh plugin can declare and do, once, on one plugin: to
see how the router draws and handles each part, to test the router with
it (its pages, its log, applies that wait or fail, safe mode, tunnels as
ways out), and as a starting point for a new plugin.

- `plugin/manifest.toml` — what the router installs.
- `crates/demo` — the service (`bin/demo`) and its tests.
- `kmod/demo` — its kernel module: the device kind `demo`.
- `build.toml` — targets and the module; `cargo xtask` builds, packs and
  tests in QEMU.
- `.github/workflows/build.yml` — a tag `v<version>` releases to the feed.

```bash
cargo test
cargo xtask build
```

A release: set `version` in `plugin/manifest.toml`, tag `v<version>`, push.

## What it has

**A module.** `demo.ko` registers the device kind `demo` (rtnetlink): a
device that drops what is routed into it and counts it, like `dummy`.
The router loads it at install and at boot, before the network; it says
`demo: loaded` in the kernel's log, and each device says when it is made
and removed. While safe mode holds the plugin back the module is not
loaded and the plugin is `Held back`.

**A page** (`/plugins/demo`). Its settings have every type of field
(text with a pattern, secret, template, bytes, bool, the three integers,
range, duration, port, host and port, both prefixes, address, choice,
the four references, lists) in five sections, one folded; nothing is
done with them but counting them (`Fields set`) and showing `address`
back. The settings that change what it does:

| Setting        | What it does |
|----------------|--------------|
| `greeting`     | shown as a value, logged by Say hello and the heartbeat |
| `log_level`    | `debug`: a line for every converge (every second) |
| `heartbeat`    | a line in the log this often, counted |
| `settle_after` | `settled` only this long after a new `desired`: an apply waits for it, and fails past `settle_within` (30 s) |

`records` is a group keyed by `host`: each row has its pings, the last
ping's time and a summary of its uplink and port, is `down` unless
`enabled` is ticked (an absent key is off) and in
`error` after Break or mend (pressed again, it is up). The page's
buttons: Say hello (a notice), Log 100 lines (twenty of each level),
Take 5 seconds, Fail (always fails). A value of every type is shown:
text, count, bytes, duration, time and address.

**A tunnel**, kind `demo` (`[tunnels.<key>.demo]`): a `demo` device with
an IPv4 address (and an IPv6 one), the MTU 1420 unless set.

| Setting       | What it does |
|---------------|--------------|
| `endpoint`    | pinged over UDP every keepalive from a socket with the tunnel's mark, the answers counted: `connecting` until answered and after three keepalives without, `up` while answered |
| `keepalive`   | how often (5 s) |
| `listen_port` | opened in the firewall on the policy's uplinks; every ping heard there is answered |
| `simulate`    | reported instead of its phase: `error` fails an apply, `starting` never settles, `connecting` and `down` are shown |

Two routers whose demo tunnels name each other's endpoint and listen
port answer each other; one with an endpoint nobody answers stays
`connecting`. Its values: pings sent, answers, pings heard, bytes sent,
the last answer, the keepalive and what the endpoint resolved to.

**Its log** (`logread -e demo`): every `desired` it is told, every change
of a tunnel's phase (`demo/<key>`), every button, the heartbeat, and why
a device or a port could not be had.
