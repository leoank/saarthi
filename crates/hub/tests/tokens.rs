//! Phase 2 step 3 end to end on loopback (ADR-0039, ADR-0042): a hub holding two named
//! Claude tokens lists them by name with their dates and never their values; a spawn runs
//! under the node's own login until the node has a default, then under the default, or
//! under a token picked or the own login picked; an unknown token is refused before the
//! node is asked; a resume keeps the token the session last ran under; the account windows
//! a session's stream reports land on its token; expiry is set by hand and cleared. Usage
//! probes (ADR-0043) keep every account's windows fresh without listing a session, and only
//! a node that runs headless sessions takes them.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use common::{HubProc, start_spawning_node};
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::*;
use saarthi_proto::event::v1::Session;
use tonic::transport::Channel;

const HOLD: Duration = Duration::from_secs(30);
const TOKENS: &str = "work tok-work\npersonal tok-personal\n";
const YEAR_MS: i64 = 365 * 24 * 3600 * 1000;

/// Records the token it was started with, then answers each `user` line with the
/// rate-limit event claude streams and a `result`; exits on EOF.
const FAKE: &str = r#"#!/bin/sh
echo "token=${CLAUDE_CODE_OAUTH_TOKEN:-none}" >> LOG
while IFS= read -r line; do
  case "$line" in
    *'"type":"user"'*)
      echo '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","unifiedWindows":{"five_hour":{"utilization":0.39,"resetsAt":1792255200},"seven_day":{"utilization":0.17,"resetsAt":1792753200}}}}'
      echo '{"type":"result","subtype":"success"}' ;;
  esac
done
"#;

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut()
        .insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

async fn connected(c: &mut ControlServiceClient<Channel>) -> Node {
    loop {
        let nodes = c.list_nodes(req(ListNodesRequest {})).await.unwrap().into_inner().nodes;
        if let Some(n) = nodes.into_iter().find(|n| n.connected) {
            return n;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn session(c: &mut ControlServiceClient<Channel>, id: &str, ended: bool) -> Session {
    for _ in 0..250 {
        let all = c
            .list_sessions(req(ListSessionsRequest {
                node_id: String::new(),
                include_ended: true,
            }))
            .await
            .unwrap()
            .into_inner()
            .sessions;
        if let Some(s) = all.into_iter().find(|s| s.id == id && s.ended_ms.is_some() == ended) {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("session {id} never showed with ended={ended}");
}

/// The `token=` lines the fake claude has written so far.
fn starts(log: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix("token=").map(str::to_string))
        .collect()
}

async fn nth_start(log: &std::path::Path, n: usize) -> String {
    for _ in 0..250 {
        if let Some(t) = starts(log).get(n) {
            return t.clone();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("start {n} never reached the fake claude");
}

async fn tokens(c: &mut ControlServiceClient<Channel>) -> Vec<ClaudeToken> {
    c.list_claude_tokens(req(ListClaudeTokensRequest {}))
        .await
        .unwrap()
        .into_inner()
        .tokens
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_run_under_named_tokens_chosen_or_by_default_and_report_their_usage() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("work");
    std::fs::create_dir_all(root.join("repo")).unwrap();
    let log = dir.path().join("starts.log");
    let fake = dir.path().join("claude");
    std::fs::write(&fake, FAKE.replace("LOG", &format!("'{}'", log.display()))).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let claude_dir = dir.path().join(".claude");

    let (hub, addr) = HubProc::start_full(&dir.path().join("hub.db"), "127.0.0.1:0", None, TOKENS);
    let _node = start_spawning_node(
        &addr,
        "t-node",
        &claude_dir,
        HOLD,
        &fake.to_string_lossy(),
        std::slice::from_ref(&root),
        Default::default(),
    )
    .await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let node = connected(&mut c).await;
    assert_eq!(node.default_claude_token, "");
    let cwd = root.canonicalize().unwrap().join("repo").to_string_lossy().into_owned();
    let spawn = |claude_token: Option<&str>| SpawnRequest {
        node_id: node.id.clone(),
        cwd: cwd.clone(),
        prompt: "hello".into(),
        model: String::new(),
        create: false,
        permission_mode: String::new(),
        profile: String::new(),
        claude_token: claude_token.map(str::to_string),
    };

    // Names and dates, never values.
    let listed = tokens(&mut c).await;
    assert_eq!(listed.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["personal", "work"]);
    let work = &listed[1];
    assert_eq!(work.expires_ms, work.first_seen_ms + YEAR_MS);
    assert!(!work.expiry_set);
    assert_eq!(work.usage_ms, 0);
    assert!(!format!("{listed:?}").contains("tok-"));

    // No default yet: the node's own login.
    let r = c.spawn(req(spawn(None))).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    assert_eq!(nth_start(&log, 0).await, "none");
    assert_eq!(session(&mut c, &r.session_id, false).await.claude_token, "");

    // A default: every spawn that picks nothing runs under it.
    let unknown = c
        .set_default_claude_token(req(SetDefaultClaudeTokenRequest {
            node_id: node.id.clone(),
            name: "nope".into(),
        }))
        .await
        .unwrap_err();
    assert_eq!(unknown.code(), tonic::Code::NotFound);
    c.set_default_claude_token(req(SetDefaultClaudeTokenRequest {
        node_id: node.id.clone(),
        name: "work".into(),
    }))
    .await
    .unwrap();
    assert_eq!(connected(&mut c).await.default_claude_token, "work");
    let by_default = c.spawn(req(spawn(None))).await.unwrap().into_inner();
    assert!(by_default.ok, "{}", by_default.error);
    assert_eq!(nth_start(&log, 1).await, "tok-work");
    assert_eq!(session(&mut c, &by_default.session_id, false).await.claude_token, "work");

    // Picked: another token, or the own login despite the default.
    let picked = c.spawn(req(spawn(Some("personal")))).await.unwrap().into_inner();
    assert!(picked.ok, "{}", picked.error);
    assert_eq!(nth_start(&log, 2).await, "tok-personal");
    let own = c.spawn(req(spawn(Some("")))).await.unwrap().into_inner();
    assert!(own.ok, "{}", own.error);
    assert_eq!(nth_start(&log, 3).await, "none");

    // An unknown token is refused by the hub; nothing starts.
    let err = c.spawn(req(spawn(Some("nope")))).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    assert!(err.message().contains("nope"), "{}", err.message());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(starts(&log).len(), 4);

    // The default session's stream reported the account's windows: they are work's.
    let mut seen = None;
    for _ in 0..250 {
        let t = tokens(&mut c).await;
        if t[1].usage_ms > 0 {
            seen = Some(t);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let t = seen.expect("work's usage from the session's rate_limit_event");
    assert_eq!(
        (t[1].five_hour_pct, t[1].five_hour_resets_s, t[1].seven_day_pct, t[1].seven_day_resets_s),
        (Some(39.0), Some(1792255200), Some(17.0), Some(1792753200))
    );

    // A resume keeps the token the session last ran under, unless another is picked.
    let id = by_default.session_id.clone();
    let transcript = saarthi_node::sessions::transcript_path(&claude_dir, &cwd, &id);
    std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    std::fs::write(&transcript, "").unwrap();
    let kill = |id: &str| KillRequest {
        node_id: node.id.clone(),
        session_id: id.to_string(),
    };
    assert!(c.kill(req(kill(&id))).await.unwrap().into_inner().ok);
    session(&mut c, &id, true).await;
    let resume = |claude_token: Option<&str>| ResumeRequest {
        node_id: node.id.clone(),
        session_id: id.clone(),
        prompt: "go on".into(),
        model: String::new(),
        claude_token: claude_token.map(str::to_string),
    };
    let r = c.resume(req(resume(None))).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    assert_eq!(nth_start(&log, 4).await, "tok-work");
    assert!(c.kill(req(kill(&id))).await.unwrap().into_inner().ok);
    session(&mut c, &id, true).await;
    let r = c.resume(req(resume(Some("personal")))).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    assert_eq!(nth_start(&log, 5).await, "tok-personal");
    assert_eq!(session(&mut c, &id, false).await.claude_token, "personal");

    // Expiry by hand, then cleared.
    let at = 1_900_000_000_000;
    let set = c
        .set_claude_token_expiry(req(SetClaudeTokenExpiryRequest {
            name: "work".into(),
            expires_ms: at,
        }))
        .await
        .unwrap()
        .into_inner()
        .tokens;
    assert_eq!((set[1].expires_ms, set[1].expiry_set), (at, true));
    let cleared = c
        .set_claude_token_expiry(req(SetClaudeTokenExpiryRequest {
            name: "work".into(),
            expires_ms: 0,
        }))
        .await
        .unwrap()
        .into_inner()
        .tokens;
    assert_eq!((cleared[1].expires_ms, cleared[1].expiry_set), (cleared[1].first_seen_ms + YEAR_MS, false));
    let missing = c
        .set_claude_token_expiry(req(SetClaudeTokenExpiryRequest {
            name: "nope".into(),
            expires_ms: at,
        }))
        .await
        .unwrap_err();
    assert_eq!(missing.code(), tonic::Code::NotFound);
    hub.stop();
}

/// Answers a `-p` probe at once: records the token, streams the windows and a result.
const PROBE_FAKE: &str = r#"#!/bin/sh
echo "probe ${CLAUDE_CODE_OAUTH_TOKEN:-none} $(basename "$CLAUDE_CONFIG_DIR")" >> LOG
echo '{"type":"system","subtype":"init"}'
echo '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","unifiedWindows":{"five_hour":{"utilization":0.04,"resetsAt":1792255200},"seven_day":{"utilization":0.36,"resetsAt":1792753200}}}}'
echo '{"type":"result","subtype":"success","is_error":false}'
"#;

#[tokio::test(flavor = "multi_thread")]
async fn usage_probes_keep_every_account_fresh_without_listing_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let log = dir.path().join("probes.log");
    let fake = dir.path().join("claude");
    std::fs::write(&fake, PROBE_FAKE.replace("LOG", &format!("'{}'", log.display()))).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

    let (hub, addr) = HubProc::start_probing(
        &dir.path().join("hub.db"),
        "127.0.0.1:0",
        TOKENS,
        "test-node",
        // Short, so the loop ticks often enough for a test; a second round may come.
        Duration::from_secs(4),
    );
    let _node = start_spawning_node(
        &addr,
        "t-node",
        &dir.path().join(".claude"),
        HOLD,
        &fake.to_string_lossy(),
        std::slice::from_ref(&root),
        Default::default(),
    )
    .await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let mut both = None;
    for _ in 0..250 {
        let t = tokens(&mut c).await;
        if t.iter().all(|t| t.usage_ms > 0) {
            both = Some(t);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let t = both.expect("both accounts probed");
    for t in &t {
        assert_eq!((t.five_hour_pct, t.seven_day_pct), (Some(4.0), Some(36.0)), "{}", t.name);
    }
    // Each under its own token, in a config directory of its own, and nothing listed.
    let mut lines: Vec<String> = std::fs::read_to_string(&log).unwrap().lines().map(str::to_string).collect();
    lines.sort();
    lines.dedup();
    assert_eq!(lines, ["probe tok-personal claude", "probe tok-work claude"]);
    let sessions = c
        .list_sessions(req(ListSessionsRequest {
            node_id: String::new(),
            include_ended: true,
        }))
        .await
        .unwrap()
        .into_inner()
        .sessions;
    assert!(sessions.is_empty(), "{sessions:?}");
    // The probe's directory is gone.
    let left: Vec<_> = std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("saarthi-probe-"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    hub.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_node_without_spawn_roots_takes_no_probes() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, addr) = HubProc::start_probing(
        &dir.path().join("hub.db"),
        "127.0.0.1:0",
        TOKENS,
        "test-node",
        Duration::from_secs(4),
    );
    // The stand-in that would answer a probe, so only the missing roots can stop one.
    let fake = dir.path().join("claude");
    std::fs::write(&fake, PROBE_FAKE.replace("LOG", &format!("'{}'", dir.path().join("probes.log").display()))).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let _node = start_spawning_node(
        &addr,
        "t-node",
        &dir.path().join(".claude"),
        HOLD,
        &fake.to_string_lossy(),
        &[],
        Default::default(),
    )
    .await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    connected(&mut c).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(tokens(&mut c).await.iter().all(|t| t.usage_ms == 0));
    hub.stop();
}
