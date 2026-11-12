//! Keeps the machine from idle-sleeping while a session is busy (ADR-0031, ADR-0045): a
//! child process holds the machine up while any known session is busy and is dropped when
//! none is, so a laptop whose screen turned off still finishes the turn and keeps its hub
//! link. On macOS the child is `caffeinate -i`; on Linux it is `systemd-inhibit` holding a
//! blocking `idle:sleep` lock around `cat`, which logind grants only where polkit allows
//! it, so the node checks once at start. Where no hold is possible the node reports no
//! keep-awake metrics and a client offers no switch. The state goes out as the health
//! metric `node.keep_awake` (1 while held), and whether the hold is switched on as
//! `node.keep_awake_enabled`. Switched on at start by `--keep-awake`, and from a client at
//! any time (ADR-0034).

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use saarthi_proto::event::v1::SessionStatus;

use saarthi_node::sessions::Sessions;

/// Whether the hold is on, shared with the health sampler.
pub type Held = Arc<AtomicBool>;

/// What holds the machine up.
#[derive(Clone, Debug)]
pub enum Hold {
    /// macOS: `caffeinate -i`.
    Caffeinate,
    /// Linux: this `systemd-inhibit`.
    Inhibit(PathBuf),
}

const INHIBIT: [&str; 4] = ["--what=idle:sleep", "--who=saarthi", "--mode=block", "--why"];

impl Hold {
    /// The hold this machine allows, if any: on Linux a `systemd-inhibit` found on PATH
    /// that logind lets this user take (no polkit, or a rule that refuses, means none).
    pub async fn detect() -> Option<Self> {
        if cfg!(target_os = "macos") {
            return Some(Self::Caffeinate);
        }
        if !cfg!(target_os = "linux") {
            return None;
        }
        let path = std::env::var_os("PATH")?;
        let inhibit = std::env::split_paths(&path)
            .map(|d| d.join("systemd-inhibit"))
            .find(|p| p.is_file())?;
        Self::check(inhibit).await
    }

    /// Takes the lock once around `true`: the hold is offered only if that succeeds.
    pub async fn check(inhibit: PathBuf) -> Option<Self> {
        let out = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::process::Command::new(&inhibit)
                .args(INHIBIT)
                .arg("checking that keep-awake can hold")
                .arg("true")
                .stdin(Stdio::null())
                .output(),
        )
        .await;
        match out {
            Ok(Ok(o)) if o.status.success() => Some(Self::Inhibit(inhibit)),
            Ok(Ok(o)) => {
                let why = String::from_utf8_lossy(&o.stderr).trim().to_string();
                tracing::info!(error = %why, "keep-awake unavailable: logind refused the lock (a polkit rule allows it)");
                None
            }
            Ok(Err(e)) => {
                tracing::info!(error = %e, "keep-awake unavailable: systemd-inhibit did not run");
                None
            }
            Err(_) => {
                tracing::info!("keep-awake unavailable: systemd-inhibit did not answer");
                None
            }
        }
    }

    /// The child that holds the machine up for as long as it runs. Dropping it kills it;
    /// on Linux `cat` also ends when the node's end of its stdin closes, so neither child
    /// outlives the node.
    fn spawn(&self) -> std::io::Result<tokio::process::Child> {
        let mut cmd = match self {
            Self::Caffeinate => {
                let mut c = tokio::process::Command::new("/usr/bin/caffeinate");
                c.arg("-i").stdin(Stdio::null());
                c
            }
            Self::Inhibit(inhibit) => {
                let mut c = tokio::process::Command::new(inhibit);
                c.args(INHIBIT)
                    .arg("a Claude Code session is working")
                    .arg("cat")
                    .stdin(Stdio::piped());
                c
            }
        };
        cmd.stdout(Stdio::null()).kill_on_drop(true).spawn()
    }
}

/// Watches the sessions and holds or releases the machine while `enabled`; runs until the
/// node exits.
pub async fn keep(
    hold: Hold,
    sessions: Arc<Sessions>,
    enabled: Arc<AtomicBool>,
    held: Held,
    every: Duration,
) {
    let mut child: Option<tokio::process::Child> = None;
    let mut tick = tokio::time::interval(every);
    loop {
        tick.tick().await;
        let busy = enabled.load(Ordering::Relaxed)
            && sessions
                .snapshot()
                .await
                .iter()
                .any(|s| s.ended_ms.is_none() && s.status() == SessionStatus::Busy);
        match (busy, child.as_mut()) {
            (true, None) => match hold.spawn() {
                Ok(c) => {
                    child = Some(c);
                    held.store(true, Ordering::Relaxed);
                    tracing::info!("keeping the machine awake: a session is busy");
                }
                Err(e) => tracing::warn!(error = %e, ?hold, "the keep-awake hold failed to start"),
            },
            (false, Some(c)) => {
                let _ = c.start_kill();
                let _ = c.wait().await;
                child = None;
                held.store(false, Ordering::Relaxed);
                tracing::info!("letting the machine sleep: no session is busy, or keep-awake is off");
            }
            (true, Some(c)) => {
                // The child died (killed by hand, or the lock was refused): take it again
                // on the next tick.
                if let Ok(Some(_)) = c.try_wait() {
                    child = None;
                    held.store(false, Ordering::Relaxed);
                }
            }
            (false, None) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saarthi_node::outbox::Outbox;
    use saarthi_node::registry::Record;
    use std::os::unix::fs::PermissionsExt;

    /// A stand-in `systemd-inhibit`: records its arguments, refuses when told to, then runs
    /// the command it was given, as the real one does.
    const FAKE: &str = r#"#!/bin/sh
echo "$*" >> LOG
[ -e REFUSE ] && { echo "Failed to inhibit: Access denied" >&2; exit 1; }
shift 5
exec "$@"
"#;

    fn fake(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("systemd-inhibit");
        let script = FAKE
            .replace("LOG", &dir.join("log").display().to_string())
            .replace("REFUSE", &dir.join("refuse").display().to_string());
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    async fn until(what: &str, mut done: impl FnMut() -> bool) {
        for _ in 0..200 {
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("never {what}");
    }

    #[tokio::test]
    async fn a_refused_lock_offers_no_hold() {
        let dir = tempfile::tempdir().unwrap();
        let inhibit = fake(dir.path());
        std::fs::write(dir.path().join("refuse"), "").unwrap();
        assert!(Hold::check(inhibit.clone()).await.is_none());
        std::fs::remove_file(dir.path().join("refuse")).unwrap();
        assert!(matches!(Hold::check(inhibit).await, Some(Hold::Inhibit(_))));
    }

    #[tokio::test]
    async fn the_lock_is_held_while_a_session_is_busy_and_only_then() {
        let dir = tempfile::tempdir().unwrap();
        let hold = Hold::check(fake(dir.path())).await.unwrap();
        let (outbox, _rx) = Outbox::new(64);
        let sessions = Arc::new(Sessions::new(
            dir.path().join(".claude"),
            outbox,
            Duration::from_millis(10),
            Vec::new(),
        ));
        let (enabled, held) = (Arc::new(AtomicBool::new(true)), Held::default());
        tokio::spawn(keep(
            hold,
            sessions.clone(),
            enabled.clone(),
            held.clone(),
            Duration::from_millis(20),
        ));
        let record = |status: &str| Record {
            pid: std::process::id(),
            session_id: "s1".into(),
            cwd: "/w".into(),
            started_at: 1,
            status: status.into(),
            messaging_socket_path: String::new(),
            claude_dir: None,
            profile: String::new(),
        };
        let log = dir.path().join("log");
        let holds = || {
            std::fs::read_to_string(&log)
                .unwrap_or_default()
                .lines()
                .filter(|l| l.contains("a Claude Code session is working"))
                .count()
        };

        sessions.apply_registry(&[record("busy")]).await;
        until("held while busy", || held.load(Ordering::Relaxed)).await;
        let line = std::fs::read_to_string(&log).unwrap();
        assert!(line.contains("--what=idle:sleep --who=saarthi --mode=block"), "{line}");
        assert_eq!(holds(), 1);

        sessions.apply_registry(&[record("idle")]).await;
        until("released when idle", || !held.load(Ordering::Relaxed)).await;

        // Switched off, a busy session takes no lock.
        enabled.store(false, Ordering::Relaxed);
        sessions.apply_registry(&[record("busy")]).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(!held.load(Ordering::Relaxed));
        assert_eq!(holds(), 1);
    }
}
