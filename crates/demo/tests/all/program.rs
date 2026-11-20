//! `demo <socket>` as procd runs it.
use crate::support::manifest;
use routeh_plugin_sdk::testing::{Router, TempDir};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn program() -> Command {
    Command::new(env!("CARGO_BIN_EXE_demo"))
}

/// A child killed and reaped when dropped, so a failed assertion leaves
/// no process behind.
struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn the_program_takes_its_socket_and_nothing_else() {
    assert_eq!(program().status().unwrap().code(), Some(2));
    let status = program()
        .arg("/nonexistent-root-dir/x/y.sock")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
}

/// In ujail the service is PID 1 of its namespace, where a signal it does
/// not handle is ignored: it ends on SIGTERM itself and takes its socket
/// away.
#[test]
fn the_program_says_hello_and_ends_on_sigterm() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("demo.sock");
    let mut child = Running(program().arg(&path).spawn().unwrap());
    let mut router = Router::connect(&path, &manifest()).unwrap();
    let hello = router.hello().unwrap();
    assert_eq!((hello.plugin.as_str(), hello.protocol), ("demo", 1));
    let killed = Command::new("kill")
        .args(["-TERM", &child.0.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(started.elapsed() < Duration::from_secs(5), "it did not end");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(0));
    assert!(!path.exists());
}
