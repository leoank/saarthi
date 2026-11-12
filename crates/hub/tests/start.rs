//! A hub whose Claude tokens file does not parse still starts (ADR-0044): it runs with no
//! Claude tokens, so sessions run under each node's own login until the file is fixed.

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
fn a_hub_with_a_bad_claude_tokens_file_still_serves() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("nodes"), "n t-node\n").unwrap();
    std::fs::write(d.join("clients"), "c t-client\n").unwrap();
    // a name without a token
    std::fs::write(d.join("claude"), "work\n").unwrap();
    let addr = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let mut hub = Kill(
        Command::new(env!("CARGO_BIN_EXE_saarthi-hub"))
            .args(["--listen", &addr.to_string()])
            .arg("--db")
            .arg(d.join("hub.db"))
            .arg("--node-tokens")
            .arg(d.join("nodes"))
            .arg("--client-tokens")
            .arg(d.join("clients"))
            .arg("--claude-tokens")
            .arg(d.join("claude"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    while std::net::TcpStream::connect(addr).is_err() {
        assert!(hub.0.try_wait().unwrap().is_none(), "the hub exited");
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "the hub never served"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(300));
    assert!(hub.0.try_wait().unwrap().is_none(), "the hub exited");
}
