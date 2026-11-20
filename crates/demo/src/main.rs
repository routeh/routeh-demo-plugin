//! `demo <socket>`: the demo plugin's service, with the kernel's own
//! devices and sockets. The SDK takes the socket, logs to the system log
//! and ends on SIGTERM.
fn main() -> std::process::ExitCode {
    routeh_plugin_sdk::run(demo::Demo::system())
}
