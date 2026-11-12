//! A node whose optional settings are wrong still starts (ADR-0044): a missing spawn root is
//! created, one that cannot be is left out, and a profiles file that does not parse leaves
//! the node without profiles. The daemon serves its hook endpoint and keeps running.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Kill(Child);

impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn a_node_with_a_missing_root_and_a_bad_profiles_file_still_serves() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    std::fs::write(home.join("a-file"), "").unwrap();
    std::fs::write(home.join("profiles.toml"), "this is = not [toml").unwrap();
    let missing = home.join("missing/workspace");
    let addr = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let mut node = Kill(
        Command::new(env!("CARGO_BIN_EXE_saarthi"))
            .args(["node", "serve", "--hook-addr", &addr.to_string()])
            .arg("--claude-dir")
            .arg(home.join(".claude"))
            .arg("--wrapper-socket")
            .arg(home.join("w.sock"))
            .arg("--spawn-root")
            .arg(&missing)
            // under a regular file: it can never be created
            .arg("--spawn-root")
            .arg(home.join("a-file/workspace"))
            .arg("--profiles")
            .arg(home.join("profiles.toml"))
            .env("HOME", home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    while std::net::TcpStream::connect(addr).is_err() {
        assert!(node.0.try_wait().unwrap().is_none(), "the node exited");
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "the node never served"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(300));
    assert!(node.0.try_wait().unwrap().is_none(), "the node exited");
    assert!(missing.is_dir(), "the missing root was not created");
}
