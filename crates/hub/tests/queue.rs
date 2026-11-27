//! Prompts queued while a session works (phase PHS-20261117-f22kxe item 7), end to end on
//! loopback with a stand-in claude: a prompt sent with `queue` to a busy session is held by
//! the node and listed in the session's `queue` event; one is discarded, the other goes to
//! claude when its turn ends; taking a prompt the node no longer holds says so.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use common::{HubProc, start_spawning_node};
use saarthi_hub::db::Db;
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::*;
use serde_json::Value;

/// Logs every input line; answers a `user` line with a `result` at once, or for one that
/// says "slow" only once the file GO exists. Exits on EOF.
const FAKE: &str = r#"#!/bin/sh
while IFS= read -r line; do
  echo "$line" >> LOG
  # Whatever order the line's keys come in (serde_json keeps the written order where a
  # dependency asks it to, as cedar-policy does).
  case "$line" in
    *'"type":"user"'*)
      case "$line" in *slow*) while [ ! -f GO ]; do sleep 0.05; done ;; esac
      echo '{"type":"result","subtype":"success"}' ;;
  esac
done
"#;

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut().insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

async fn until<T>(what: &str, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    for _ in 0..400 {
        if let Some(v) = f().await {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("never {what}");
}

/// The texts in the session's latest `queue` event, with their ids.
async fn queued(db: &Db, node_id: &str, id: &str) -> Option<Vec<(String, String)>> {
    let events = db.session_events(node_id, id, 0, 10_000).await.unwrap();
    let last = events.iter().rev().find(|e| e.kind == "queue")?;
    let v: Value = serde_json::from_str(&last.payload_json).unwrap();
    Some(
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| (q["id"].as_str().unwrap().to_string(), q["text"].as_str().unwrap().to_string()))
            .collect(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn prompts_queued_while_a_session_works_go_when_it_is_idle() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let log = dir.path().join("input.log");
    let go = dir.path().join("go");
    let fake = dir.path().join("claude");
    let script = FAKE.replace("LOG", &format!("'{}'", log.display())).replace("GO", &format!("'{}'", go.display()));
    std::fs::write(&fake, script).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let db_path = dir.path().join("hub.db");
    let (hub, addr) = HubProc::start(&db_path, "127.0.0.1:0");
    let _node = start_spawning_node(
        &addr,
        "t-node",
        &dir.path().join(".claude"),
        Duration::from_secs(30),
        &fake.to_string_lossy(),
        std::slice::from_ref(&root),
        Default::default(),
    )
    .await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let node_id = until("a node connected", async || {
        let nodes = c.list_nodes(req(ListNodesRequest {})).await.unwrap().into_inner().nodes;
        nodes.into_iter().find(|n| n.connected).map(|n| n.id)
    })
    .await;
    let spawn = SpawnRequest {
        node_id: node_id.clone(),
        cwd: root.canonicalize().unwrap().to_string_lossy().into_owned(),
        prompt: "slow work".into(),
        ..Default::default()
    };
    let r = c.spawn(req(spawn)).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    let id = r.session_id;
    let read = || std::fs::read_to_string(&log).unwrap_or_default();
    until("the first prompt reached claude", async || read().contains("slow work").then_some(())).await;

    let prompt = |text: &str| PromptRequest {
        node_id: node_id.clone(),
        session_id: id.clone(),
        text: text.into(),
        attachments: Vec::new(),
        queue: true,
    };
    for text in ["then the docs", "and a changelog"] {
        let r = c.prompt(req(prompt(text))).await.unwrap().into_inner();
        assert!(r.ok && r.queued, "{} queued={}", r.error, r.queued);
    }
    let db = Db::open(&db_path).await.unwrap();
    let held = until("both listed", async || queued(&db, &node_id, &id).await.filter(|q| q.len() == 2)).await;
    assert_eq!(held.iter().map(|q| q.1.as_str()).collect::<Vec<_>>(), ["then the docs", "and a changelog"]);
    assert!(!read().contains("then the docs"), "held, not handed to claude");

    // Discard the second; the first goes when the turn ends.
    let discard = UnqueueRequest { node_id: node_id.clone(), session_id: id.clone(), queued_id: held[1].0.clone(), send: false };
    let r = c.unqueue(req(discard.clone())).await.unwrap().into_inner();
    assert!(r.ok && r.note.is_empty(), "{} {}", r.error, r.note);
    std::fs::write(&go, "").unwrap();
    until("the queued prompt reached claude", async || read().contains("then the docs").then_some(())).await;
    until("the queue emptied", async || queued(&db, &node_id, &id).await.filter(|q| q.is_empty())).await;
    assert!(!read().contains("and a changelog"), "the discarded prompt never goes");

    // Taking it again: not there any more, and the person is told so.
    let r = c.unqueue(req(discard)).await.unwrap().into_inner();
    assert!(r.ok && r.note.contains("no longer queued"), "{} {}", r.error, r.note);

    // An idle session takes a prompt sent with `queue` at once.
    let r = c.prompt(req(prompt("one more"))).await.unwrap().into_inner();
    assert!(r.ok && !r.queued, "{}", r.error);
    until("the prompt reached claude", async || read().contains("one more").then_some(())).await;
    hub.stop();
}
